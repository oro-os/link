use core::ptr::addr_of_mut;

use crate::chip::Transact;
use crate::uc;
use defmt::{debug, info};
use embassy_executor::Spawner;
use embassy_net::driver::Driver;
use embassy_stm32::mode::Async;
use embassy_stm32::{
	bind_interrupts,
	exti::ExtiInput,
	gpio::{Input, Level, Output, OutputOpenDrain, Pull, Speed},
	i2c::{self, I2c},
	peripherals, rcc, rng, rtc,
	spi::{self, Spi},
	time::Hertz,
	usart, usb as stm32_usb, Config,
};
use embassy_time::{Delay, Duration, Timer};
use embassy_usb as usb;
use embedded_hal_bus::spi::ExclusiveDevice;

bind_interrupts!(struct Irqs {
	I2C1_EV => i2c::EventInterruptHandler<peripherals::I2C1>;
	I2C1_ER => i2c::ErrorInterruptHandler<peripherals::I2C1>;
	USART3 => usart::InterruptHandler<peripherals::USART3>;
	HASH_RNG => rng::InterruptHandler<peripherals::RNG>;
	OTG_FS => stm32_usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

type EthernetSPI = ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>;

#[embassy_executor::task]
async fn syseth_runner_task(
	runner: embassy_net_wiznet::Runner<
		'static,
		embassy_net_wiznet::chip::W5500,
		EthernetSPI,
		ExtiInput<'static>,
		Output<'static>,
	>,
) -> ! {
	runner.run().await
}

#[embassy_executor::task]
async fn exteth_runner_task(
	runner: embassy_net_wiznet::Runner<
		'static,
		embassy_net_wiznet::chip::W5500,
		EthernetSPI,
		ExtiInput<'static>,
		Output<'static>,
	>,
) -> ! {
	runner.run().await
}

pub async fn init<'usb>(
	spawner: &Spawner,
) -> (
	impl uc::DebugLed,
	impl uc::SystemUnderTest,
	impl uc::Monitor,
	impl uc::EthernetDriver,
	//impl uc::EthernetDriver,
	impl uc::WallClock,
	impl uc::Rng,
	impl uc::UartTx,
	impl uc::UartRx,
	impl uc::PacketTracer,
	impl uc::UniqueId,
	impl uc::ResetManager,
	usb::Builder<'usb, impl usb::driver::Driver<'usb>>,
) {
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

	let p = embassy_stm32::init(config);

	info!("initializing STM32f479vg...");
	Timer::after(Duration::from_millis(100)).await;

	// Blink
	{
		let mut pd2 = Output::new(p.PD2, Level::Low, Speed::Low);
		loop {
			pd2.set_high();
			Timer::after(Duration::from_millis(300)).await;
			pd2.set_low();
			Timer::after(Duration::from_millis(300)).await;
		}
	}

	let mut ind_on = Output::new(p.PB4, Level::Low, Speed::Low);
	ind_on.set_high();
	::core::mem::forget(ind_on); // Keep it high even after we return.

	let i2c = I2c::new(
		p.I2C1,
		p.PB6,
		p.PB7,
		Irqs,
		p.DMA1_CH6,
		p.DMA1_CH5,
		Hertz(400_000),
		Default::default(),
	);

	let indicators = crate::chip::is31fl3218::Is31fl3218::new(i2c);
	let indicators =
		crate::chip::is31fl3218::IndicatorLights::<_, 0, 1, 17, 12, 13, 11, 16, 14, 15>::new(
			indicators,
		);

	info!("... indicators INIT");

	let wall_clock = rtc::Rtc::new(p.RTC, rtc::RtcConfig::default());
	info!("... rtc INIT");

	// Let OLED power on (affects first power-on cycle, typically)
	Timer::after(Duration::from_millis(100)).await;

	let mut oled_en = Output::new(p.PE2, Level::Low, Speed::Low);
	let mut oled_rst = Output::new(p.PC13, Level::Low, Speed::Low);
	oled_en.set_low();
	oled_rst.set_low();
	Timer::after(Duration::from_millis(100)).await;
	oled_en.set_high();
	Timer::after(Duration::from_millis(100)).await;
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
			Spi::new_txonly(p.SPI2, p.PD3, p.PC3, p.DMA1_CH4, oledconf),
			OutputOpenDrain::new(p.PB9, Level::High, Speed::VeryHigh),
			Delay,
		)
		.unwrap(),
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

	//let mut syseth_en = Output::new(p.PA2, Level::Low, Speed::Low);
	//let mut syseth_xfrm_en = Output::new(p.PA3, Level::Low, Speed::Low);
	//syseth_en.set_high();
	//syseth_xfrm_en.set_high();
	//// Keep them high even after we return.
	//::core::mem::forget(syseth_en);
	//::core::mem::forget(syseth_xfrm_en);

	//info!("... system ethernet transformer INIT");

	//let mut sysconf = spi::Config::default();
	////sysconf.mode = spi::MODE_0;
	////sysconf.bit_order = spi::BitOrder::MsbFirst;
	//sysconf.frequency = Hertz(50_000_000);

	//let sysspi = Spi::new(p.SPI1, p.PA5, p.PA7, p.PA6, p.DMA2_CH3, p.DMA2_CH0, sysconf);

	//info!("... system ethernet comms INIT");

	//let sysdev = ExclusiveDevice::new(
	//	sysspi,
	//	Output::new(p.PA4, Level::High, Speed::VeryHigh),
	//	Delay,
	//)
	//.unwrap();

	//info!("... system ethernet dev INIT");

	//let syseth_mac_addr = [b'.', b'o', b'O', b'D', b'E', b'V'];

	//let syseth = {
	//	static STATE: static_cell::StaticCell<embassy_net_wiznet::State<2, 2>> =
	//		static_cell::StaticCell::new();
	//	let state = STATE.init(embassy_net_wiznet::State::<2, 2>::new());
	//	let intpin = ExtiInput::new(p.PB0, p.EXTI0, Pull::Up);
	//	let rstpin = Output::new(p.PB1, Level::High, Speed::VeryHigh);
	//	let (syseth, runner) =
	//		embassy_net_wiznet::new(syseth_mac_addr, state, sysdev, intpin, rstpin)
	//			.await
	//			.unwrap();

	//	spawner.must_spawn(syseth_runner_task(runner));

	//	syseth
	//};

	//info!("... system ethernet INIT");

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
	extconf.frequency = Hertz(40_000_000);

	let extspi = Spi::new(
		p.SPI3, p.PC10, p.PC12, p.PC11, p.DMA1_CH7, p.DMA1_CH2, extconf,
	);

	info!("... external ethernet comms INIT");

	let mut extdev = ExclusiveDevice::new(
		extspi,
		Output::new(p.PA15, Level::High, Speed::VeryHigh),
		Delay,
	)
	.unwrap();

	info!("... external ethernet dev INIT");

	let extmac = super::get_exteth_mac();

	debug!("... external MAC: {:?}", extmac);

	static EXT_STATE: static_cell::StaticCell<embassy_net_wiznet::State<2, 2>> =
		static_cell::StaticCell::new();
	let ext_state = EXT_STATE.init(embassy_net_wiznet::State::<2, 2>::new());
	let ext_intpin = ExtiInput::new(p.PD1, p.EXTI1, Pull::Up);
	let mut ext_rstpin = Output::new(p.PD0, Level::High, Speed::VeryHigh);
	let ext_rstpin_fake = Output::new(p.PD9, Level::Low, Speed::VeryHigh);

	Timer::after_millis(100).await;
	ext_rstpin.set_low();
	Timer::after_millis(100).await;
	ext_rstpin.set_high();
	Timer::after_millis(100).await;

	// let out_buf = [0, 0x1cu8, 0, 0];
	// let mut in_buf = [0u8; 4];

	// loop {
	// 	let r = extdev.transact(&out_buf, &mut in_buf);
	// 	debug!("transaction on: {:?} {:?}", r.is_ok(), in_buf);
	// 	in_buf = [0, 0, 0, 0];
	// 	Timer::after_millis(500).await;
	// 	ext_rstpin.set_low();
	// 	Timer::after_millis(10).await;
	// 	let r = extdev.transact(&out_buf, &mut in_buf);
	// 	debug!("transaction off: {:?} {:?}", r.is_ok(), in_buf);
	// 	in_buf = [0, 0, 0, 0];
	// 	Timer::after_millis(10).await;
	// 	ext_rstpin.set_high();
	// 	Timer::after_millis(500).await;
	// }

	let (exteth, ext_runner) =
		embassy_net_wiznet::new(extmac, ext_state, extdev, ext_intpin, ext_rstpin_fake)
			.await
			.unwrap();

	spawner.must_spawn(exteth_runner_task(ext_runner));

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

	let rng_gen = rng::Rng::new(p.RNG, Irqs);

	info!("... rng INIT");

	let mut syscom_config = usart::Config::default();
	syscom_config.baudrate = 38400;
	syscom_config.data_bits = usart::DataBits::DataBits8;
	syscom_config.stop_bits = usart::StopBits::STOP1;
	syscom_config.parity = usart::Parity::ParityNone;

	// TODO maybe expose this somehow so that the test runner can turn it on and off.
	let mut syscom_on = Output::new(p.PD8, Level::Low, Speed::Low);
	syscom_on.set_high();
	::core::mem::forget(syscom_on); // Keep it high even after we return.

	let (syscom_tx, syscom_rx) = usart::Uart::new_with_rtscts(
		p.USART3,
		p.PB11,
		p.PB10,
		Irqs,
		p.PB14,
		p.PB13,
		p.DMA1_CH3,
		p.DMA1_CH1,
		syscom_config,
	)
	.expect("failed to create SUT usart pair")
	.split();

	const DMA_BUF_SIZE: usize = 256;
	static mut DMA_BUF: [u8; DMA_BUF_SIZE] = [0; DMA_BUF_SIZE];
	let syscom_rx = syscom_rx.into_ring_buffered(unsafe { DMA_BUF.as_mut() });

	info!("... system com INIT");

	let mut auxcom_config = usart::Config::default();
	auxcom_config.baudrate = 38400;
	auxcom_config.data_bits = usart::DataBits::DataBits8;
	auxcom_config.stop_bits = usart::StopBits::STOP1;
	auxcom_config.parity = usart::Parity::ParityNone;
	auxcom_config.assume_noise_free = false;

	let auxcom_tx = usart::UartTx::new_blocking(p.UART7, p.PE8, auxcom_config)
		.expect("failed to create aux uart pair");

	info!("... aux com INIT");

	let uid = super::StmUniqueId;

	info!("... uid INIT");

	let rst = super::CortexResetManager;

	info!("... reset manager INIT");

	static mut EP_OUT_BUFFER: [u8; 256] = [0u8; 256];

	let mut config = stm32_usb::Config::default();
	config.vbus_detection = true;

	let driver = stm32_usb::Driver::new_fs(
		p.USB_OTG_FS,
		Irqs,
		p.PA12,
		p.PA11,
		unsafe { &mut *addr_of_mut!(EP_OUT_BUFFER) },
		config,
	);

	let mut usb_config = usb::Config::new(0x1337, 0x9001);
	usb_config.manufacturer = Some("Oro Operating System");
	usb_config.product = Some("Oro Link");
	usb_config.serial_number = Some("this-is-just-a-test-device");
	usb_config.max_power = 0;
	usb_config.max_packet_size_0 = 64;

	// Required for windows compatibility.
	// https://developer.nordicsemi.com/nRF_Connect_SDK/doc/1.9.1/kconfig/CONFIG_CDC_ACM_IAD.html#help
	usb_config.device_class = 0xEF;
	usb_config.device_sub_class = 0x02;
	usb_config.device_protocol = 0x01;
	usb_config.composite_with_iads = true;

	static mut CONFIG_DESCRIPTOR: [u8; 256] = [0; 256];
	static mut BOS_DESCRIPTOR: [u8; 256] = [0; 256];
	// You can also add a Microsoft OS descriptor.
	static mut MSOS_DESCRIPTOR: [u8; 256] = [0; 256];
	static mut CONTROL_BUF: [u8; 64] = [0; 64];

	let usb_builder = unsafe {
		usb::Builder::new(
			driver,
			usb_config,
			&mut *addr_of_mut!(CONFIG_DESCRIPTOR),
			&mut *addr_of_mut!(BOS_DESCRIPTOR),
			&mut *addr_of_mut!(MSOS_DESCRIPTOR),
			&mut *addr_of_mut!(CONTROL_BUF),
		)
	};

	info!("... usb INIT");

	(
		debug_led,
		system,
		monitor,
		exteth,
		//syseth,
		wall_clock,
		rng_gen,
		syscom_tx,
		syscom_rx,
		auxcom_tx,
		uid,
		rst,
		usb_builder,
	)
}
