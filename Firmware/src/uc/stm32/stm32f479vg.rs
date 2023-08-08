use crate::uc;
use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::{
	bind_interrupts,
	dma::NoDma,
	gpio::{Input, Level, Output, OutputOpenDrain, Pull, Speed},
	i2c::{self, I2c},
	peripherals,
	spi::{self, Spi},
	time::Hertz,
	usart::{self, Uart},
	Config,
};
use embassy_time::Delay;
use embedded_hal_bus::spi::ExclusiveDevice;

bind_interrupts!(struct Irqs {
	I2C1_EV => i2c::InterruptHandler<peripherals::I2C1>;
	UART7 => usart::InterruptHandler<peripherals::UART7>;
});

pub async fn init(
	spawner: &Spawner,
) -> (
	impl uc::DebugLed,
	impl uc::SystemUnderTest,
	impl uc::Monitor,
	impl uc::EthernetDriver,
) {
	let mut config = Config::default();
	config.rcc.hse = Some(Hertz::mhz(26));
	config.rcc.bypass_hse = false;
	config.rcc.hclk = Some(Hertz(168409091));
	config.rcc.sys_ck = Some(Hertz(168409091));
	config.rcc.pll48 = true;

	let p = embassy_stm32::init(config);

	let debug_write = Uart::new(p.UART7, p.PE7, p.PE8, Irqs, NoDma, NoDma, {
		let mut config = usart::Config::default();
		config.baudrate = 115200;
		config.data_bits = usart::DataBits::DataBits8;
		config.stop_bits = usart::StopBits::STOP1;
		config.parity = usart::Parity::ParityNone;
		config
	});

	super::start_defmt_task(spawner, debug_write);

	info!("initializing STM32f479vg...");

	let mut ind_on = Output::new(p.PB4, Level::Low, Speed::Low);
	ind_on.set_high();
	::core::mem::forget(ind_on); // Keep it high even after we return.

	let i2c = I2c::new(
		p.I2C1,
		p.PB6,
		p.PB7,
		Irqs,
		NoDma,
		NoDma,
		Hertz(400_000),
		Default::default(),
	);

	let indicators = crate::chip::is31fl3218::Is31fl3218::new(i2c);
	let indicators =
		crate::chip::is31fl3218::IndicatorLights::<_, 0, 1, 17, 12, 13, 11, 16, 14, 15>::new(
			indicators,
		);

	info!("... indicators INIT");

	let mut oled_en = Output::new(p.PE2, Level::Low, Speed::Low);
	let mut oled_rst = Output::new(p.PC13, Level::Low, Speed::Low);
	oled_en.set_high();
	oled_rst.set_high();
	// Keep it high even after we return
	::core::mem::forget(oled_en);
	::core::mem::forget(oled_rst);

	let mut oledconf = spi::Config::default();
	oledconf.mode = spi::MODE_0;
	oledconf.bit_order = spi::BitOrder::MsbFirst;
	oledconf.frequency = Hertz(8_000_000);

	let mut oled = crate::chip::ssd1362::SSD1362::new(
		ExclusiveDevice::new(
			Spi::new_txonly(p.SPI2, p.PD3, p.PC3, NoDma, NoDma, oledconf),
			OutputOpenDrain::new(p.PB9, Level::High, Speed::VeryHigh, Pull::None),
			Delay,
		),
		Output::new(p.PC14, Level::High, Speed::VeryHigh),
		true, // do a flip
		137,  // gamma value
	)
	.unwrap();

	oled.on().unwrap();
	oled.clear().unwrap();

	info!("... oled INIT");

	let monitor =
		crate::uc::helper::monitor::three_indicators_oled_256x64::ThreeIndicatorsOled256x64::new(
			oled, indicators,
		);

	info!("... monitor INIT");

	let mut exteth_en = Output::new(p.PD7, Level::Low, Speed::Low);
	let mut exteth_xfrm_en = Output::new(p.PD2, Level::Low, Speed::Low);
	exteth_en.set_high();
	exteth_xfrm_en.set_high();
	// Keep them high even after we return.
	::core::mem::forget(exteth_en);
	::core::mem::forget(exteth_xfrm_en);

	info!("... external ethernet transformer INIT");

	let mut extconf = spi::Config::default();
	extconf.mode = spi::MODE_0;
	extconf.bit_order = spi::BitOrder::MsbFirst;
	extconf.frequency = Hertz(8_000_000);

	let extspi = Spi::new(p.SPI3, p.PC10, p.PC12, p.PC11, NoDma, NoDma, extconf);

	info!("... external ethernet comms INIT");

	let extdev = ExclusiveDevice::new(
		extspi,
		OutputOpenDrain::new(p.PA15, Level::High, Speed::VeryHigh, Pull::None),
		Delay,
	);

	info!("... external ethernet dev INIT");

	let exteth = crate::chip::enc28j60::Enc28j60::new(
		extdev,
		Some(Output::new(p.PD0, Level::High, Speed::VeryHigh)),
		[b'.', b'o', b'O', b'D', b'E', b'V'],
	);

	info!("... external ethernet INIT");

	let system = super::SystemUnderTest::new(
		Output::new(p.PC9, Level::Low, Speed::Low),
		Output::new(p.PC8, Level::Low, Speed::Low),
		Output::new(p.PD6, Level::Low, Speed::Low),
		Output::new(p.PD4, Level::Low, Speed::Low),
		Input::new(p.PD5, Pull::Up),
	);

	info!("... system under test INIT");

	let debug_led = super::DebugLed::new(Output::new(p.PE12, Level::Low, Speed::Low));

	info!("... debug led INIT");

	(debug_led, system, monitor, exteth)
}
