#![no_std]
#![no_main]
#![feature(never_type)]
#![feature(adt_const_params)]
#![feature(unsafe_cell_access)]

pub(crate) mod atomic;
pub(crate) mod channel;
pub(crate) mod color;
pub(crate) mod crc32;
// pub(crate) mod flash;
pub(crate) mod font;
pub(crate) mod nvram;
pub(crate) mod rand;
pub(crate) mod service;
pub(crate) mod unique_id;
pub(crate) mod version;
pub(crate) mod wol;

use core::{cell::UnsafeCell, net::Ipv4Addr};

use defmt_rtt as _;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_stm32::{
	Config, bind_interrupts, dma,
	exti::{self, ExtiInput},
	gpio::{Input, Level, Output, OutputOpenDrain, Pull, Speed},
	i2c, interrupt,
	mode::{Async, Blocking},
	peripherals, rcc, rng, spi,
	time::Hertz,
	usart, usb,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex, once_lock::OnceLock};
use embassy_time::{Delay, Duration, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use panic_probe as _;
use static_cell::StaticCell;

use crate::{
	nvram::{LastBootFailure, Volatile, VolatileLastBootFailure},
	service::svc_mqtt_stats::{BoolStat, Stat, StrStat},
};

pub static STAT_INITIALIZED: BoolStat = BoolStat::new("status/initialized");
pub static STAT_VERSION_MAJOR: StrStat<u64, 4> = StrStat::new("version/major");
pub static STAT_VERSION_MINOR: StrStat<u64, 4> = StrStat::new("version/minor");
pub static STAT_VERSION_PATCH: StrStat<u64, 4> = StrStat::new("version/patch");
pub static STAT_LAST_BOOT_FAILURE: Stat<LastBootFailure> = Stat::new("status/boot_failure");
pub static STAT_AUX_VBUS_SENSE: BoolStat = BoolStat::new("power/aux_vbus_sense");

bind_interrupts!(struct Irqs {
	OTG_HS => usb::InterruptHandler<peripherals::USB_OTG_HS>;
	USART2 => usart::InterruptHandler<peripherals::USART2>;
	UART7 => usart::InterruptHandler<peripherals::UART7>;
	HASH_RNG => rng::InterruptHandler<peripherals::RNG>;
	DMA1_STREAM0 => dma::InterruptHandler<peripherals::DMA1_CH0>;
	DMA1_STREAM1 => dma::InterruptHandler<peripherals::DMA1_CH1>;
	DMA1_STREAM3 => dma::InterruptHandler<peripherals::DMA1_CH3>;
	DMA1_STREAM4 => dma::InterruptHandler<peripherals::DMA1_CH4>;
	DMA1_STREAM5 => dma::InterruptHandler<peripherals::DMA1_CH5>;
	DMA1_STREAM6 => dma::InterruptHandler<peripherals::DMA1_CH6>;
	DMA1_STREAM7 => dma::InterruptHandler<peripherals::DMA1_CH7>;
	DMA2_STREAM0 => dma::InterruptHandler<peripherals::DMA2_CH0>;
	DMA2_STREAM1 => dma::InterruptHandler<peripherals::DMA2_CH1>;
	EXTI15_10 => exti::InterruptHandler<interrupt::typelevel::EXTI15_10>;
	EXTI9_5 => exti::InterruptHandler<interrupt::typelevel::EXTI9_5>;
	EXTI4 => exti::InterruptHandler<interrupt::typelevel::EXTI4>;
	EXTI0 => exti::InterruptHandler<interrupt::typelevel::EXTI0>;
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
	// defmt::debug!("initializing flash");
	// flash::init_pflash(p.FLASH);

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

	let last_boot_failure = nv_ram.failure.take_and_reset();
	match last_boot_failure {
		nvram::LastBootFailure::None => defmt::info!("last boot failure: {:?}", last_boot_failure),
		other => defmt::error!("last boot failure: {:?}", other),
	}
	STAT_LAST_BOOT_FAILURE.set(last_boot_failure);

	// let pflash = match flash::read_pflash() {
	// 	Ok(pflash) => pflash,
	// 	Err(e) => {
	// 		defmt::warn!("failed to read pflash; reinitializing: {:?}", e);
	// 		// SAFETY: We're initializing it.
	// 		unsafe { flash::write_pflash(flash::Pflash::default()) }
	// 			.expect("failed to write/read-back default pflash")
	// 	}
	//};

	// let mut pflash = pflash.into_latest();
	// defmt::debug!("pflash contents: {:?}", pflash);

	if nv_ram.reboot.fast_count.read() >= 10 {
		nv_ram.reset();

		defmt::warn!(
			"detected {} fast reboots; starting Oro Link in initialization mode",
			nv_ram.reboot.fast_count
		);

		// pflash.initialized = false;

		// if let Err(err) = unsafe { flash::write_pflash(pflash) } {
		// 	defmt::error!(
		// 		"failed to reset pflash during fast reboot recovery; system is NOT reset: {:?}",
		// 		err
		// 	);
		//}

		defmt::warn!("Oro Link reset complete; rebooting system");

		// SAFETY: We're resetting the system.
		unsafe { self::reset() }
	}

	// let initialized = if pflash.initialized {
	// 	defmt::info!("system is initialized");
	// 	true
	//} else {
	// 	defmt::warn!("system is uninitialized; performing first-time setup");
	// 	nv_ram.reset();
	// 	false
	//};
	let initialized = true;

	STAT_INITIALIZED.set(initialized);
	STAT_VERSION_MAJOR.set(crate::version::VERSION_MAJOR);
	STAT_VERSION_MINOR.set(crate::version::VERSION_MINOR);
	STAT_VERSION_PATCH.set(crate::version::VERSION_PATCH);

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
		p.PA1,
		p.PD3,
		p.DMA1_CH6,
		p.DMA1_CH5,
		Irqs,
		{
			let mut config = usart::Config::default();
			config.baudrate = 115_200;
			config
		},
	)
	.unwrap();

	let usb_output_selector = Output::new(p.PA7, Level::High, Speed::Low);
	let ulpi_oc = ExtiInput::new(p.PB14, p.EXTI14, Pull::None, Irqs);
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

	static SPI3: StaticCell<Mutex<NoopRawMutex, spi::Spi<'static, Async, spi::mode::Master>>> =
		StaticCell::new();
	let spi3 = SPI3.init(Mutex::new(spi::Spi::new(
		p.SPI3,
		p.PC10,
		p.PC12,
		p.PC11,
		p.DMA1_CH7,
		p.DMA1_CH0,
		Irqs,
		{
			let mut config = spi::Config::default();
			config.frequency = Hertz(22_500_000);
			config.bit_order = spi::BitOrder::MsbFirst;
			config.mode = spi::MODE_0;
			config.miso_pull = Pull::Up;
			config
		},
	)));

	let sd_oc = ExtiInput::new(p.PA6, p.EXTI6, Pull::None, Irqs);
	let sd_sense = ExtiInput::new(p.PC13, p.EXTI13, Pull::None, Irqs);
	let _sd_sense_cable = ExtiInput::new(p.PD8, p.EXTI8, Pull::None, Irqs);
	// TODO(qix-): Switch back to open drain after pullup is added
	// let sd_en = OutputOpenDrain::new(p.PC14, Level::High, Speed::Low);
	let sd_en = OutputOpenDrain::new_pull(p.PC14, Level::High, Speed::Low, Pull::Up);
	let sd_cs = OutputOpenDrain::new_pull(p.PD5, Level::High, Speed::VeryHigh, Pull::Up);
	let sd_host_sut_sel = Output::new(p.PD14, Level::Low, Speed::Low);
	let sd_spi: &'static _ = spi3;

	let syseth_int = ExtiInput::new(p.PA4, p.EXTI4, Pull::None, Irqs);
	let mut syseth_rst = Output::new(p.PC15, Level::Low, Speed::VeryHigh);
	let syseth_cs = OutputOpenDrain::new(p.PD7, Level::High, Speed::VeryHigh);
	let syseth = SpiDevice::new(spi3, syseth_cs);
	let syseth_seed = self::rand::next_u64();
	reset_wiznet_chip(&mut syseth_rst).await;

	static SYSETH_STATE: StaticCell<embassy_net_wiznet::State<2, 2>> = StaticCell::new();
	let syseth_state = SYSETH_STATE.init(embassy_net_wiznet::State::<2, 2>::new());
	let (syseth_driver, syseth_wiznet_runner) = embassy_net_wiznet::new(
		syseth_mac_address(),
		syseth_state,
		syseth,
		syseth_int,
		syseth_rst,
	)
	.await
	.unwrap();

	static SYSETH_STACK: StaticCell<embassy_net::StackResources<16>> = StaticCell::new();
	let syseth_stack_resources = SYSETH_STACK.init(embassy_net::StackResources::<16>::new());
	let (_syseth_stack, syseth_net_runner) = embassy_net::new(
		syseth_driver,
		embassy_net::Config::ipv4_static({
			let mut cfg = embassy_net::StaticConfigV4 {
				address:     embassy_net::Ipv4Cidr::new(Ipv4Addr::from_octets([10, 0, 0, 1]), 8),
				dns_servers: Default::default(),
				gateway:     None,
			};
			cfg.dns_servers
				.push(Ipv4Addr::from_octets([10, 0, 0, 1]))
				.unwrap();
			cfg
		}),
		syseth_stack_resources,
		syseth_seed,
	);

	let (_uart_tx, uart_rx) =
		usart::Uart::new(p.UART7, p.PE7, p.PE8, p.DMA1_CH1, p.DMA1_CH3, Irqs, {
			let mut config = usart::Config::default();
			config.baudrate = 3_000_000;
			config.stop_bits = usart::StopBits::STOP1;
			config.parity = usart::Parity::ParityNone;
			config
		})
		.unwrap()
		.split();

	static UART_RX_BUFFER: StaticCell<[u8; 4096]> = StaticCell::new();
	let uart_rx_buffer = UART_RX_BUFFER.init([0u8; 4096]);
	let _uart_rx = uart_rx.into_ring_buffered(uart_rx_buffer);

	let exteth_int = ExtiInput::new(p.PA0, p.EXTI0, Pull::None, Irqs);
	let mut exteth_int_polarity = OutputOpenDrain::new(p.PB6, Level::Low, Speed::Low);
	exteth_int_polarity.set_low(); // Enable ethernet interrupt polarity (active low)
	let mut exteth_rst = OutputOpenDrain::new(p.PD0, Level::High, Speed::VeryHigh);
	let exteth_cs = OutputOpenDrain::new(p.PE11, Level::High, Speed::VeryHigh);
	let exteth = spi::Spi::new(
		p.SPI4,
		p.PE2,
		p.PE14,
		p.PE13,
		p.DMA2_CH1,
		p.DMA2_CH0,
		Irqs,
		{
			let mut config = spi::Config::default();
			config.frequency = Hertz(20_000_000);
			config
		},
	);
	let exteth_seed = self::rand::next_u64();
	let exteth = ExclusiveDevice::new(exteth, exteth_cs, Delay).unwrap();
	reset_wiznet_chip(&mut exteth_rst).await;

	static EXTETH_STATE: StaticCell<embassy_net_wiznet::State<2, 2>> = StaticCell::new();
	let exteth_state = EXTETH_STATE.init(embassy_net_wiznet::State::<2, 2>::new());
	let (exteth_driver, exteth_wiznet_runner) = embassy_net_wiznet::new(
		exteth_mac_address(),
		exteth_state,
		exteth,
		exteth_int,
		exteth_rst,
	)
	.await
	.unwrap();

	static EXTETH_STACK: StaticCell<embassy_net::StackResources<16>> = StaticCell::new();
	let exteth_stack_resources = EXTETH_STACK.init(embassy_net::StackResources::<16>::new());
	let (exteth_stack, exteth_net_runner) = embassy_net::new(
		exteth_driver,
		embassy_net::Config::dhcpv4(Default::default()),
		exteth_stack_resources,
		exteth_seed,
	);

	let oled_rst = Output::new(p.PD1, Level::High, Speed::Low);
	let oled_cs = OutputOpenDrain::new(p.PB9, Level::High, Speed::VeryHigh);
	let oled_dc = Output::new(p.PD4, Level::Low, Speed::VeryHigh);
	let oled_en = Output::new(p.PD9, Level::Low, Speed::Low);
	let oled = spi::Spi::new_txonly(p.SPI2, p.PA9, p.PC1, p.DMA1_CH4, Irqs, {
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

	let vbus_oc = ExtiInput::new(p.PD15, p.EXTI15, Pull::None, Irqs);
	let vbus_en = Output::new(p.PE15, Level::Low, Speed::Low);
	let aux_vbus_sense_pin = Input::new(p.PA11, Pull::None);
	// Have we sensed the aux vbus line?
	let aux_vbus_sense = aux_vbus_sense_pin.is_low();
	STAT_AUX_VBUS_SENSE.set(aux_vbus_sense);
	let aux_vbus_oc = ExtiInput::new(p.PA12, p.EXTI12, Pull::None, Irqs);
	let aux_vbus_en = OutputOpenDrain::new(p.PA15, Level::High, Speed::Low);
	// TODO: Set `Pull::None` once external pull-up has been added
	// TODO: https://github.com/oro-os/link/issues/112
	let board_power_alert = ExtiInput::new(p.PE9, p.EXTI9, Pull::Up, Irqs);
	let psu_on = Output::new(p.PD10, Level::Low, Speed::Low);
	let _sut_pwr_switch = Output::new(p.PE12, Level::Low, Speed::Low);
	let _sut_rst_switch = Output::new(p.PE10, Level::Low, Speed::Low);

	defmt::info!("initialization complete; starting services...");

	static MQTT_CELL: StaticCell<OnceLock<service::svc_mqtt::Mqtt>> = StaticCell::new();
	let mqtt: &'static _ = MQTT_CELL.init(OnceLock::new());

	static FAILURE_REF: StaticCell<UnsafeCell<&'static mut Volatile<LastBootFailure>>> =
		StaticCell::new();
	let failure = FAILURE_REF.init(UnsafeCell::new(&mut nv_ram.failure));

	service_config! {
		dev_blinken_light {
			debug_led1,
			debug_led2,
			debug_led3,
		},
		dev_exteth {
			wiznet_runner: exteth_wiznet_runner,
			net_runner: exteth_net_runner,
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
			i2c,
		},
		dev_sdcard {
			sd: sd_spi,
			sd_cs,
			sd_en,
			sd_sense,
			sd_host_sut_sel,
		},
		dev_syseth {
			wiznet_runner: syseth_wiznet_runner,
			net_runner: syseth_net_runner,
		},
		dev_usb {
			driver: ulpi,
			ulpi_rst,
		},
		svc_main {
			mqtt,
			aux_vbus_sense,
			last_boot_failure,
			usb_output_selector
		},
		svc_mqtt {
			stack: exteth_stack,
			mqtt
		},
		svc_mqtt_stats {
			spawner,
			mqtt
		},
		svc_mqtt_config {
			mqtt,
			spawner
		},
		failsafe_board_oc {
			board_power_alert,
			failure
		},
		failsafe_aux_vbus_oc {
			aux_vbus_oc,
			failure
		},
		failsafe_ulpi_oc {
			ulpi_oc,
			failure
		},
		failsafe_sd_oc {
			sd_oc,
			failure
		},
		svc_vbus_power {
			aux_vbus_sense,
			aux_vbus_en,
			vbus_en,
			vbus_oc,
			failure
		},
		svc_psu {
			psu_on
		}
	}
	.spawn_all(spawner);

	defmt::info!("all services have been spawned");

	Timer::after(Duration::from_secs(5)).await;
	defmt::debug!("marking boot as successful");
	nv_ram.reboot.reset();

	loop {
		Timer::after_secs(3600).await;
	}
}

fn exteth_mac_address() -> [u8; 6] {
	let hash = self::unique_id::unique_id_sha256();

	let mut macaddr = [0u8; 6];
	macaddr[0] = b'.';
	macaddr[1] = b'o';
	macaddr[2] = b'O';
	macaddr[3] = hash[29];
	macaddr[4] = hash[30];
	macaddr[5] = hash[31];

	macaddr
}

pub fn unique_id() -> &'static str {
	static ID: OnceLock<[u8; 6 * 2 + 5]> = OnceLock::new();
	let id = ID.get_or_init(|| {
		let mac = exteth_mac_address();
		let mut id = [b'-'; { 6 * 2 + 5 }];
		for (i, b) in mac.iter().enumerate() {
			let b = *b;
			id[i * 3] = ((b >> 4) & 0xF).hex_digit();
			id[i * 3 + 1] = (b & 0xF).hex_digit();
		}
		id
	});

	// SAFETY: We can assert it's correct.
	unsafe { core::str::from_utf8_unchecked(id) }
}

fn syseth_mac_address() -> [u8; 6] {
	let mut macaddr = [0u8; 6];
	macaddr[0] = b'.';
	macaddr[1] = b'o';
	macaddr[2] = b'O';
	macaddr[3] = 0;
	macaddr[4] = 0;
	macaddr[5] = 0;

	macaddr
}

async fn reset_wiznet_chip<P: embedded_hal::digital::OutputPin>(pin: &mut P) {
	Timer::after_millis(100).await;
	let _ = pin.set_low();
	Timer::after_millis(100).await;
	let _ = pin.set_high();
	Timer::after_millis(100).await;
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

trait HexDigit {
	fn hex_digit(self) -> u8;
}

impl HexDigit for u8 {
	fn hex_digit(self) -> u8 {
		match self {
			0..=9 => b'0' + self,
			10..=15 => b'A' + (self - 10),
			_ => b'_',
		}
	}
}
