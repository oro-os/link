#![no_std]
#![no_main]
#![feature(never_type)]

pub(crate) mod channel;
pub(crate) mod color;
pub(crate) mod font;
pub(crate) mod service;
pub(crate) mod unique_id;

use defmt::info;
use defmt_rtt as _;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_stm32::{
	Config, bind_interrupts,
	exti::ExtiInput,
	gpio::{Input, Level, Output, OutputOpenDrain, Pull, Speed},
	i2c,
	mode::{Async, Blocking},
	peripherals, rcc, spi,
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

	info!("initializing oro link...");
	Timer::after(Duration::from_millis(100)).await;

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

	let _usb_output_selector = Output::new(p.PA7, Level::Low, Speed::Low);
	let ulpi_oc = ExtiInput::new(p.PB14, p.EXTI14, Pull::None);
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

	let sd_oc = ExtiInput::new(p.PA6, p.EXTI6, Pull::None);
	let sd_sense = ExtiInput::new(p.PC13, p.EXTI13, Pull::None);
	let sd_sense_cable = ExtiInput::new(p.PD8, p.EXTI8, Pull::None);
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

	let uart = usart::Uart::new(p.UART7, p.PE7, p.PE8, Irqs, p.DMA1_CH1, p.DMA1_CH3, {
		let mut config = usart::Config::default();
		config.baudrate = 115_200;
		config.stop_bits = usart::StopBits::STOP1;
		config.parity = usart::Parity::ParityNone;
		config
	})
	.unwrap();

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

	let service_channels = service::services_channels();

	defmt::info!("service: blinken lights...");
	spawner
		.spawn(service::blinken_light::blinken_light(
			service_channels.blinken_light_rx,
			debug_led1,
			debug_led2,
			debug_led3,
		))
		.unwrap();
	defmt::info!("service: led controller...");
	spawner
		.spawn(service::led_controller::led_controller(
			spawner,
			service_channels.led_controller_rx,
			i2c,
			ind_en,
		))
		.unwrap();
	defmt::info!("service: power monitor...");
	spawner
		.spawn(service::power_monitor::power_monitor(i2c))
		.unwrap();
	defmt::info!("service: usb...");
	spawner
		.spawn(service::usb::usb_service(ulpi, ulpi_rst, ulpi_oc))
		.unwrap();
	defmt::info!("service: external ethernet...");
	spawner
		.spawn(service::exteth::exteth_service(
			exteth, exteth_cs, exteth_rst, exteth_int, 0, // TODO
		))
		.unwrap();
	defmt::info!("service: system ethernet...");
	spawner
		.spawn(service::syseth::syseth_service(
			syseth, syseth_rst, syseth_int, 0, // TODO
		))
		.unwrap();
	defmt::info!("service: oled...");
	spawner
		.spawn(service::oled::oled_service(
			oled, oled_cs, oled_dc, oled_rst, oled_en,
		))
		.unwrap();
	defmt::info!("service: sdcard...");
	spawner
		.spawn(service::sdcard::sdcard_service(
			sd_spi,
			sd_cs,
			sd_en,
			sd_oc,
			sd_sense,
			sd_sense_cable,
			sd_host_sut_sel,
		))
		.unwrap();
	defmt::info!("service: uart...");
	spawner.spawn(service::uart::uart_service(uart)).unwrap();
	defmt::info!("service: usart...");
	spawner.spawn(service::usart::usart_service(usart)).unwrap();
	defmt::info!("service: ci/cd...");
	spawner
		.spawn(service::cicd::cicd_service(
			service_channels.master_bus.sender(),
		))
		.unwrap();

	defmt::info!("link is now ready; beginning Oro Link CI/CD main routine - happy hacking!");
	service_channels.master_bus.run_master_bus().await;
}
