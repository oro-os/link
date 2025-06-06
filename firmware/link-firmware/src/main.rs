#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

//mod service;

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::Output;
use embassy_stm32::{
	Config, bind_interrupts,
	gpio::{Input, Level, OutputOpenDrain, Pull, Speed},
	i2c, peripherals, rcc, spi,
	time::Hertz,
};
use embassy_stm32::{usart, usb};
use embassy_time::{Duration, Timer};
use panic_probe as _;
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
	OTG_HS => usb::InterruptHandler<peripherals::USB_OTG_HS>;
	USART2 => usart::InterruptHandler<peripherals::USART2>;
	UART7 => usart::InterruptHandler<peripherals::UART7>;
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
		divr: None,
	});
	config.rcc.pllsai = Some(rcc::Pll {
		prediv: rcc::PllPreDiv::DIV24,
		mul: rcc::PllMul::MUL192,
		divp: None,
		divq: Some(rcc::PllQDiv::DIV4),
		divr: None,
	});

	let mut clock_mux = rcc::mux::ClockMux::default();
	clock_mux.clk48sel = rcc::mux::Clk48sel::PLLSAI1_Q;
	config.rcc.mux = clock_mux;

	config.rcc.apb1_pre = rcc::APBPrescaler::DIV4;
	config.rcc.apb2_pre = rcc::APBPrescaler::DIV2;

	config.enable_debug_during_sleep = true;

	let p = embassy_stm32::init(config);

	info!("initializing oro link...");
	Timer::after(Duration::from_millis(100)).await;

	let debug_led1 = Output::new(p.PD2, Level::Low, Speed::Low);
	let debug_led2 = Output::new(p.PB7, Level::Low, Speed::Low);
	let debug_led3 = Output::new(p.PC8, Level::Low, Speed::Low);

	let ind_en = Output::new(p.PB8, Level::Low, Speed::Low);

	let i2c = i2c::I2c::new_blocking(p.I2C3, p.PA8, p.PC9, Hertz(400_000), {
		let mut config = i2c::Config::default();
		config.scl_pullup = false;
		config.sda_pullup = false;
		config.timeout = Duration::from_millis(10);
		config
	});

	let usart = usart::Uart::new_with_rtscts(
		p.USART2,
		p.PD6,
		p.PA2,
		Irqs,
		p.PA1,
		p.PD3,
		p.DMA1_CH6,
		p.DMA1_CH5,
		{
			let mut config = usart::Config::default();
			config.baudrate = 115_200;
			config
		},
	)
	.unwrap();

	let ulpi_oc = ExtiInput::new(p.PB14, p.EXTI14, Pull::None);
	let ulpi_rst = OutputOpenDrain::new(p.PB15, Level::High, Speed::Low);
	static EP_OUT_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();
	let ep_out_buffer = EP_OUT_BUFFER.init([0; 256]);
	let ulpi = usb::Driver::new_hs_ulpi(
		p.USB_OTG_HS,
		Irqs,
		p.PA5,
		p.PC2,
		p.PC3,
		p.PC0,
		p.PA3,
		p.PB0,
		p.PB1,
		p.PB10,
		p.PB11,
		p.PB12,
		p.PB13,
		p.PB5,
		ep_out_buffer,
		{
			let mut config = usb::Config::default();
			config.vbus_detection = false;
			config.xcvrdly = true; // We're using a Microchip USB3340 PHY
			config
		},
	);

	let sd_oc = ExtiInput::new(p.PA6, p.EXTI6, Pull::None);
	let sd_sense = ExtiInput::new(p.PC13, p.EXTI13, Pull::None);
	let sd_sense_cable = ExtiInput::new(p.PD8, p.EXTI8, Pull::None);
	let sd_en = OutputOpenDrain::new(p.PC14, Level::High, Speed::Low);
	let sd_cs = OutputOpenDrain::new(p.PD5, Level::High, Speed::VeryHigh);
	let sd_host_sut_sel = Output::new(p.PD14, Level::Low, Speed::Low);
	let sd = spi::Spi::new(p.SPI3, p.PC10, p.PC12, p.PC11, p.DMA1_CH7, p.DMA1_CH0, {
		let mut config = spi::Config::default();
		config.frequency = Hertz(400_000);
		config
	});

	let syseth_int = ExtiInput::new(p.PA4, p.EXTI4, Pull::None);
	let syseth_rst = OutputOpenDrain::new(p.PC15, Level::High, Speed::VeryHigh);
	let syseth_cs = OutputOpenDrain::new(p.PD7, Level::High, Speed::VeryHigh);

	let uart = usart::Uart::new(p.UART7, p.PE7, p.PE8, Irqs, p.DMA1_CH1, p.DMA1_CH3, {
		let mut config = usart::Config::default();
		config.baudrate = 115_200;
		config.stop_bits = usart::StopBits::STOP1;
		config.parity = usart::Parity::ParityNone;
		config
	})
	.unwrap();

	let exteth_int = ExtiInput::new(p.PA0, p.EXTI0, Pull::None);
	let exteth_int_polarity = OutputOpenDrain::new(p.PB6, Level::Low, Speed::Low);
	let exteth_rst = OutputOpenDrain::new(p.PD0, Level::High, Speed::VeryHigh);
	let exteth_cs = OutputOpenDrain::new(p.PE11, Level::High, Speed::VeryHigh);
	let exteth = spi::Spi::new(p.SPI4, p.PE2, p.PE14, p.PE13, p.DMA2_CH1, p.DMA2_CH0, {
		let mut config = spi::Config::default();
		config.frequency = Hertz(50_000_000);
		config
	});

	let oled_rst = OutputOpenDrain::new(p.PD1, Level::High, Speed::Low);
	let oled_cs = OutputOpenDrain::new(p.PB9, Level::High, Speed::VeryHigh);
	let oled_dc = Output::new(p.PD4, Level::Low, Speed::VeryHigh);
	let oled_en = Output::new(p.PD9, Level::Low, Speed::Low);
	let oled = spi::Spi::new_txonly(p.SPI2, p.PA9, p.PC1, p.DMA1_CH4, {
		let mut oledconf = spi::Config::default();
		oledconf.mode = spi::MODE_0;
		oledconf.bit_order = spi::BitOrder::MsbFirst;
		oledconf.frequency = Hertz(8_000_000);
		oledconf
	});

	let gpio1 = Output::new(p.PA7, Level::Low, Speed::Low);
	let gpio2 = Output::new(p.PC7, Level::Low, Speed::Low);
	let gpio3 = Output::new(p.PA10, Level::Low, Speed::Low);
	let gpio4 = Output::new(p.PC6, Level::Low, Speed::Low);
	let gpio5 = Output::new(p.PB4, Level::Low, Speed::Low);

	let vbus_oc = ExtiInput::new(p.PD15, p.EXTI15, Pull::None);
	let vbus_en = Output::new(p.PE15, Level::High, Speed::Low);
	let aux_vbus_sense = Input::new(p.PA11, Pull::None);
	let aux_vbus_oc = ExtiInput::new(p.PA12, p.EXTI12, Pull::None);
	let aux_vbus_en = OutputOpenDrain::new(p.PA15, Level::High, Speed::Low);
	let board_power_alert = ExtiInput::new(p.PE9, p.EXTI9, Pull::None);
	let psu_on = Output::new(p.PD10, Level::Low, Speed::Low);
	let sut_pwr_switch = Output::new(p.PE12, Level::Low, Speed::Low);
	let sut_rst_switch = Output::new(p.PE10, Level::Low, Speed::Low);

	loop {
		Timer::after(Duration::from_millis(3000)).await;
	}
}
