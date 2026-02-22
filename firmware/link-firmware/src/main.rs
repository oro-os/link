#![no_std]
#![no_main]
#![feature(never_type)]

pub(crate) mod atomic;
pub(crate) mod channel;
pub(crate) mod color;
pub(crate) mod crc32;
pub(crate) mod flash;
pub(crate) mod font;
pub(crate) mod nvram;
pub(crate) mod rand;
pub(crate) mod service;
pub(crate) mod unique_id;
pub(crate) mod version;

use defmt_rtt as _;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_stm32::{
	Config, bind_interrupts,
	exti::ExtiInput,
	gpio::{Input, Level, Output, OutputOpenDrain, Pull, Speed},
	i2c,
	mode::{Async, Blocking},
	peripherals, rcc, rng, spi,
	time::Hertz,
	usart, usb,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};
use panic_probe as _;
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
	OTG_HS => usb::InterruptHandler<peripherals::USB_OTG_HS>;
	USART2 => usart::InterruptHandler<peripherals::USART2>;
	UART7 => usart::InterruptHandler<peripherals::UART7>;
	HASH_RNG => rng::InterruptHandler<peripherals::RNG>;
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
		mul:    rcc::PllMul::MUL360,
		divp:   Some(rcc::PllPDiv::DIV2),
		divq:   Some(rcc::PllQDiv::DIV2),
		divr:   None,
	});
	config.rcc.pllsai = Some(rcc::Pll {
		prediv: rcc::PllPreDiv::DIV24,
		mul:    rcc::PllMul::MUL192,
		divp:   None,
		divq:   Some(rcc::PllQDiv::DIV4),
		divr:   None,
	});

	let mut clock_mux = rcc::mux::ClockMux::default();
	clock_mux.clk48sel = rcc::mux::Clk48sel::PLLSAI1_Q;
	config.rcc.mux = clock_mux;

	config.rcc.apb1_pre = rcc::APBPrescaler::DIV4;
	config.rcc.apb2_pre = rcc::APBPrescaler::DIV2;

	config.enable_debug_during_sleep = true;

	let p = embassy_stm32::init(config);

	defmt::info!("initializing oro link...");
	Timer::after(Duration::from_millis(100)).await;

	// MUST BE FIRST: initialize RNG
	// Nvram and a few other things require this. Must be before anything else.
	let rng_gen = rng::Rng::new(p.RNG, Irqs);
	self::rand::init_rng(rng_gen);

	// MUST BE SECOND: Initialize pflash subsystem
	defmt::debug!("initializing flash");
	flash::init_pflash(p.FLASH);

	// Check for reset sequence
	defmt::debug!("initializing nvram");
	let nv_ram = nvram::init();
	defmt::debug!("nvram contents: {:?}", nv_ram);

	if nv_ram.reboot.in_progress.read() {
		nv_ram
			.reboot
			.fast_count
			.write(nv_ram.reboot.fast_count.read() + 1);

		defmt::warn!(
			"detected reboot in progress from nvram; fast_reboot_count={}",
			nv_ram.reboot.fast_count.read()
		);
	}

	defmt::debug!(
		"current fast reboot count: {}",
		nv_ram.reboot.fast_count.read()
	);

	let pflash = match flash::read_pflash() {
		Ok(pflash) => pflash,
		Err(e) => {
			defmt::warn!("failed to read pflash; reinitializing: {:?}", e);
			// SAFETY: We're initializing it.
			unsafe { flash::write_pflash(flash::Pflash::default()) }
				.expect("failed to write/read-back default pflash")
		}
	};

	let mut pflash = pflash.into_latest();
	defmt::debug!("pflash contents: {:?}", pflash);

	if nv_ram.reboot.fast_count.read() >= 10 {
		nv_ram.reset();

		defmt::warn!(
			"detected {} fast reboots; starting Oro Link in initialization mode",
			nv_ram.reboot.fast_count
		);

		pflash.initialized = false;

		if let Err(err) = unsafe { flash::write_pflash(pflash) } {
			defmt::error!(
				"failed to reset pflash during fast reboot recovery; system is NOT reset: {:?}",
				err
			);
		}

		defmt::warn!("Oro Link reset complete; rebooting system");

		// SAFETY: We're resetting the system.
		unsafe { self::reset() }
	}

	let initialized = if pflash.initialized {
		defmt::info!("system is initialized");
		true
	} else {
		defmt::warn!("system is uninitialized; performing first-time setup");

		nv_ram.reset();
		false
	};

	// This gets cleared on a successful boot later
	nv_ram.reboot.in_progress.write(true);

	// Begin initialization
	let debug_led1 = OutputOpenDrain::new(p.PD2, Level::High, Speed::Low);
	let debug_led2 = OutputOpenDrain::new(p.PB7, Level::High, Speed::Low);
	let debug_led3 = OutputOpenDrain::new(p.PC8, Level::High, Speed::Low);

	let ind_en = Output::new(p.PB8, Level::Low, Speed::Low);

	static I2C: StaticCell<Mutex<NoopRawMutex, i2c::I2c<'static, Blocking, i2c::mode::Master>>> =
		StaticCell::new();
	let i2c = I2C.init(Mutex::<NoopRawMutex, _>::new(i2c::I2c::new_blocking(
		p.I2C3,
		p.PA8,
		p.PC9,
		{
			let mut config = i2c::Config::default();
			config.scl_pullup = false;
			config.sda_pullup = false;
			config.timeout = Duration::from_millis(10);
			config.frequency = Hertz(400_000);
			config
		},
	)));

	let _usart = usart::Uart::new_with_rtscts(
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

	let _usb_output_selector = Output::new(p.PA7, Level::Low, Speed::Low);
	let _ulpi_oc = ExtiInput::new(p.PB14, p.EXTI14, Pull::None);
	let ulpi_rst = Output::new(p.PB15, Level::Low, Speed::Low);
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
			config.vbus_detection = true;
			config.xcvrdly = true; // We're using a Microchip USB3340 PHY
			config
		},
	);

	static SPI3: StaticCell<Mutex<NoopRawMutex, spi::Spi<'static, Async>>> = StaticCell::new();
	let spi3 = SPI3.init(Mutex::new(spi::Spi::new(
		p.SPI3,
		p.PC10,
		p.PC12,
		p.PC11,
		p.DMA1_CH7,
		p.DMA1_CH0,
		{
			let mut config = spi::Config::default();
			// config.frequency = Hertz(22_500_000);
			config.frequency = Hertz(200_000);
			config.bit_order = spi::BitOrder::MsbFirst;
			config.mode = spi::MODE_0;
			config.miso_pull = Pull::Up;
			config
		},
	)));

	let _sd_oc = ExtiInput::new(p.PA6, p.EXTI6, Pull::None);
	let sd_sense = ExtiInput::new(p.PC13, p.EXTI13, Pull::None);
	let _sd_sense_cable = ExtiInput::new(p.PD8, p.EXTI8, Pull::None);
	// TODO(qix-): Switch back to open drain after pullup is added
	// let sd_en = OutputOpenDrain::new(p.PC14, Level::High, Speed::Low);
	let sd_en = OutputOpenDrain::new_pull(p.PC14, Level::High, Speed::Low, Pull::Up);
	let sd_cs = OutputOpenDrain::new_pull(p.PD5, Level::High, Speed::VeryHigh, Pull::Up);
	let sd_host_sut_sel = Output::new(p.PD14, Level::Low, Speed::Low);
	let sd_spi: &'static _ = spi3;

	let syseth_int = ExtiInput::new(p.PA4, p.EXTI4, Pull::None);
	let syseth_rst = Output::new(p.PC15, Level::Low, Speed::VeryHigh);
	let syseth_cs = OutputOpenDrain::new(p.PD7, Level::High, Speed::VeryHigh);
	let syseth = SpiDevice::new(spi3, syseth_cs);

	let (uart_tx, uart_rx) =
		usart::Uart::new(p.UART7, p.PE7, p.PE8, Irqs, p.DMA1_CH1, p.DMA1_CH3, {
			let mut config = usart::Config::default();
			config.baudrate = 1_000_000;
			config.stop_bits = usart::StopBits::STOP1;
			config.parity = usart::Parity::ParityNone;
			config
		})
		.unwrap()
		.split();

	static UART_RX_BUFFER: StaticCell<[u8; 4096]> = StaticCell::new();
	let uart_rx_buffer = UART_RX_BUFFER.init([0u8; 4096]);
	let uart_rx = uart_rx.into_ring_buffered(uart_rx_buffer);

	let exteth_int = ExtiInput::new(p.PA0, p.EXTI0, Pull::None);
	let mut exteth_int_polarity = OutputOpenDrain::new(p.PB6, Level::Low, Speed::Low);
	exteth_int_polarity.set_high();
	let exteth_rst = OutputOpenDrain::new(p.PD0, Level::High, Speed::VeryHigh);
	let exteth_cs = OutputOpenDrain::new(p.PE11, Level::High, Speed::VeryHigh);
	let exteth = spi::Spi::new(p.SPI4, p.PE2, p.PE14, p.PE13, p.DMA2_CH1, p.DMA2_CH0, {
		let mut config = spi::Config::default();
		config.frequency = Hertz(50_000_000);
		config
	});

	let oled_rst = Output::new(p.PD1, Level::High, Speed::Low);
	let oled_cs = OutputOpenDrain::new(p.PB9, Level::High, Speed::VeryHigh);
	let oled_dc = Output::new(p.PD4, Level::Low, Speed::VeryHigh);
	let oled_en = Output::new(p.PD9, Level::Low, Speed::Low);
	let oled = spi::Spi::new_txonly(p.SPI2, p.PA9, p.PC1, p.DMA1_CH4, {
		let mut oledconf = spi::Config::default();
		oledconf.mode = spi::MODE_0;
		oledconf.bit_order = spi::BitOrder::MsbFirst;
		oledconf.frequency = Hertz(4_000_000);
		oledconf
	});

	let _gpio2 = Output::new(p.PC7, Level::Low, Speed::Low);
	let _gpio3 = Output::new(p.PA10, Level::Low, Speed::Low);
	let _gpio4 = Output::new(p.PC6, Level::Low, Speed::Low);
	let _gpio5 = Output::new(p.PB4, Level::Low, Speed::Low);

	let _vbus_oc = ExtiInput::new(p.PD15, p.EXTI15, Pull::None);
	let _vbus_en = Output::new(p.PE15, Level::High, Speed::Low);
	let _aux_vbus_sense = Input::new(p.PA11, Pull::None);
	let _aux_vbus_oc = ExtiInput::new(p.PA12, p.EXTI12, Pull::None);
	let _aux_vbus_en = OutputOpenDrain::new(p.PA15, Level::High, Speed::Low);
	let _board_power_alert = ExtiInput::new(p.PE9, p.EXTI9, Pull::None);
	let _psu_on = Output::new(p.PD10, Level::Low, Speed::Low);
	let _sut_pwr_switch = Output::new(p.PE12, Level::Low, Speed::Low);
	let _sut_rst_switch = Output::new(p.PE10, Level::Low, Speed::Low);

	defmt::info!("initialization complete; starting services...");

	service_config! {
		dev_blinken_light {
			debug_led1,
			debug_led2,
			debug_led3,
		},
		dev_exteth {
			driver: exteth,
			cs: exteth_cs,
			rst: exteth_rst,
			exti: exteth_int,
			seed: self::rand::next_u64(),
		},
		dev_leds {
			spawner,
			i2c,
			enable_chip: ind_en,
		},
		dev_oled {
			spi: oled,
			cs: oled_cs,
			dc: oled_dc,
			rst: oled_rst,
			vreg_en: oled_en,
		},
		dev_power_monitor {
			i2c
		},
		dev_sdcard {
			sd: sd_spi,
			sd_cs,
			sd_en,
			sd_sense,
			sd_host_sut_sel,
		},
		dev_syseth {
			driver: syseth,
			rst: syseth_rst,
			exti: syseth_int,
			seed: self::rand::next_u64(),
		},
		dev_usb {
			driver: ulpi,
			ulpi_rst,
		},
		svc_successful_boot {
			reboot: &mut nv_ram.reboot,
		},
		svc_main {
			initialized
		},
		svc_init {
			pflash
		},
		dev_uart {
			uart_tx,
			uart_rx,
		},
	}
	.spawn_all(spawner);

	defmt::info!("all services have been spawned");

	loop {
		Timer::after_secs(3600).await;
	}
}

/// # Safety
/// This will immediately reset the system. Use with caution.
pub unsafe fn reset() -> ! {
	defmt::warn!("performing system reset!");
	cortex_m::peripheral::SCB::sys_reset();
	#[expect(unreachable_code)]
	{
		panic!("system reset failed");
	}
}
