#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

mod service;

use defmt::{error, info};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::gpio::Output;
use embassy_stm32::{
	Config, bind_interrupts,
	gpio::{Level, OutputOpenDrain, Speed},
	i2c, peripherals, rcc,
	time::Hertz,
};
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use panic_probe as _;
use static_cell::make_static;

bind_interrupts!(struct Irqs {
	I2C3_EV => i2c::EventInterruptHandler<peripherals::I2C3>;
	I2C3_ER => i2c::ErrorInterruptHandler<peripherals::I2C3>;
});

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
	// Initialize the chip's clock
	let mut config = Config::default();
	config.rcc.ls.rtc = rcc::RtcClockSource::LSI;
	config.rcc.hse = Some(rcc::Hse {
		freq: Hertz::mhz(24),
		mode: rcc::HseMode::Oscillator,
	});
	config.rcc.pll_src = rcc::PllSource::HSE;
	config.rcc.ahb_pre = rcc::AHBPrescaler::DIV1;
	config.rcc.sys = rcc::Sysclk::PLL1_P;
	config.rcc.pll = Some(rcc::Pll {
		prediv: rcc::PllPreDiv::DIV24,
		mul: rcc::PllMul::MUL360,
		divp: Some(rcc::PllPDiv::DIV2),
		divq: Some(rcc::PllQDiv::DIV2),
		divr: Some(rcc::PllRDiv::DIV2),
	});

	let mut clock_mux = rcc::mux::ClockMux::default();
	clock_mux.clk48sel = rcc::mux::Clk48sel::PLL1_Q;
	clock_mux.dsisel = rcc::mux::Dsisel::DSI_PHY;
	clock_mux.sdiosel = rcc::mux::Sdiosel::CLK48;

	config.rcc.mux = clock_mux;

	config.rcc.apb1_pre = rcc::APBPrescaler::DIV4;
	config.rcc.apb2_pre = rcc::APBPrescaler::DIV2;

	config.enable_debug_during_sleep = true;

	let p = embassy_stm32::init(config);

	info!("initializing oro link...");
	Timer::after(Duration::from_millis(100)).await;

	// XXX only needed to work around the EN line pulled down erroneously.
	info!("turning off power-only VBUS");
	let enable_usrpwronly_vbus = Output::new(p.PD10, Level::High, Speed::Low);

	// Debug LED blink
	let pd2 = OutputOpenDrain::new(p.PD2, Level::Low, Speed::High);
	spawner.must_spawn(service::blinken_light(pd2));
	info!("started blinken light");

	// I2C
	let mut i2c = {
		make_static!(Mutex::new(i2c::I2c::new(
			p.I2C3,
			p.PA8,
			p.PC9,
			Irqs,
			p.DMA1_CH4,
			p.DMA1_CH2,
			Hertz(400_000),
			{
				let mut config = i2c::Config::default();
				config.scl_pullup = false;
				config.sda_pullup = false;
				config.timeout = Duration::from_millis(10);
				config
			}
		)))
	};

	// LED Controller
	let mut enable_lighting_controller = Output::new(p.PB8, Level::Low, Speed::Low);
	spawner.must_spawn(service::led_controller(i2c, enable_lighting_controller));
	info!("started led controller");

	// Power Monitor
	spawner.must_spawn(service::power_monitor(i2c));
	info!("started power monitor");

	Timer::after(Duration::from_millis(100)).await;

	loop {
		Timer::after(Duration::from_millis(3000)).await;
	}
}
