use core::cell::RefCell;
use embedded_hal::blocking::delay::DelayMs as DelayMsTrait;
use enc28j60::{smoltcp_phy::Phy, Enc28j60};
use stm32f4xx_hal::{
	gpio::{Input, OpenDrain, Output, Pin, PinState},
	hal::spi::MODE_0,
	i2c,
	pac::{self, I2C1, RCC, SPI1, SPI3, TIM1, UART7},
	prelude::*,
	serial::{Serial, Tx},
	spi::Spi,
	timer::DelayMs,
};

pub struct Stm32f479;

type Stm32f479DebugSerial = Tx<UART7, u8>;

static mut SHARED_DELAY: Option<RefCell<DelayMs<TIM1>>> = None;

struct SharedDelay;

impl DelayMsTrait<u8> for SharedDelay {
	fn delay_ms(&mut self, ms: u8) {
		if let Some(ref mut sd) = unsafe { &mut SHARED_DELAY } {
			sd.borrow_mut().delay_ms(ms)
		}
	}
}

impl super::Arch for Stm32f479 {
	type DebugLedImpl = Stm32f479DebugLed;
	type DebugSerialImpl = Stm32f479DebugSerial;
	type IndicatorLightsImpl = Stm32f479IndicatorLights;
	type SystemUnderTestImpl = Stm32f479SystemUnderTest;
	type ExternalEthernetDeviceImpl =
		Phy<'static, Spi<SPI3>, Pin<'A', 15, Output<OpenDrain>>, Pin<'D', 1>, Pin<'D', 0, Output>>;
	type SystemEthernetDeviceImpl =
		Phy<'static, Spi<SPI1>, Pin<'A', 4, Output<OpenDrain>>, Pin<'B', 0>, Pin<'B', 1, Output>>;

	unsafe fn initialize(
		config: super::ArchConfig,
	) -> (
		Self::DebugLedImpl,
		Self::DebugSerialImpl,
		Self::IndicatorLightsImpl,
		Self::SystemUnderTestImpl,
		super::EthernetInterfaces<Self::ExternalEthernetDeviceImpl, Self::SystemEthernetDeviceImpl>,
	) {
		let p = pac::Peripherals::take().unwrap();
		let mut syscfg = p.SYSCFG.constrain();

		init_clock(&p.RCC);
		let clocks = p.RCC.constrain().cfgr.freeze();

		unsafe {
			SHARED_DELAY = Some(RefCell::new(p.TIM1.delay_ms(&clocks)));
		}

		let gpioa = p.GPIOA.split();
		let gpiob = p.GPIOB.split();
		let gpioc = p.GPIOC.split();
		let gpiod = p.GPIOD.split();
		let gpioe = p.GPIOE.split();

		let indlights_scl = gpiob.pb6.into_alternate_open_drain();
		let indlights_sda = gpiob.pb7.into_alternate_open_drain();
		let indlights_en = gpiob.pb4.into_push_pull_output(); // TODO set to open-drain

		let mut exteth_miso = gpioc.pc11.into_alternate();
		let mut exteth_mosi = gpioc.pc12.into_alternate();
		let mut exteth_ss = gpioa.pa15.into_open_drain_output();
		let mut exteth_sck = gpioc.pc10.into_alternate();
		let mut exteth_rst = gpiod.pd0.into_push_pull_output();
		let mut exteth_int = gpiod.pd1.into_input();
		let mut exteth_en = gpiod.pd7.into_push_pull_output();
		let mut exteth_xfrm_en = gpiod.pd2.into_push_pull_output();

		let mut syseth_miso = gpioa.pa6.into_alternate();
		let mut syseth_mosi = gpioa.pa7.into_alternate();
		let mut syseth_ss = gpioa.pa4.into_open_drain_output();
		let mut syseth_sck = gpioa.pa5.into_alternate();
		let mut syseth_rst = gpiob.pb1.into_push_pull_output();
		let mut syseth_int = gpiob.pb0.into_input();
		let mut syseth_en = gpioa.pa2.into_push_pull_output();
		let mut syseth_xfrm_en = gpioa.pa3.into_push_pull_output();

		//let mut oled_mosi = gpioc.pc3.into_alternate();
		//let mut oled_ss = gpiob.pb9.into_alternate();
		//let mut oled_sck = gpiod.pd3.into_alternate();
		//let mut oled_rst = gpioc.pc13.into_push_pull_output();
		//let mut oled_dc = gpioc.pc14.into_push_pull_output();
		//let mut oled_en = gpioe.pe2.into_push_pull_output();

		//let mut uart_rx = gpioe.pe7.into_alternate();
		let uart_tx = gpioe.pe8.into_alternate();

		//let mut rs232_cts = gpiob.pb13.into_alternate();
		//let mut rs232_rts = gpiob.pb14.into_alternate();
		//let mut rs232_rx = gpiob.pb11.into_alternate();
		//let mut rs232_tx = gpiob.pb10.into_alternate();
		//let mut rs232_en = gpiod.pd8.into_push_pull_output();

		//let mut usb_dn = gpioa.pa11.into_alternate();
		//let mut usb_dp = gpioa.pa12.into_alternate();

		let sys_power = gpioc.pc8.into_push_pull_output();
		let sys_reset = gpioc.pc9.into_push_pull_output();

		let psu_standby = gpiod.pd4.into_push_pull_output();
		let psu_on = gpiod.pd6.into_push_pull_output();
		let psu_ok = gpiod.pd5.into_input();

		let dbgled = gpioe.pe12.into_push_pull_output();

		let indicator_lights_iface = p.I2C1.i2c(
			(indlights_scl, indlights_sda),
			i2c::Mode::fast(400000.Hz(), i2c::DutyCycle::Ratio2to1),
			&clocks,
		);

		let ext_eth_spi = p.SPI3.spi(
			(exteth_sck, exteth_miso, exteth_mosi),
			MODE_0,
			10.MHz(), // TODO set to 20MHz and test (this is what the datasheet specifies)
			&clocks,
		);

		let ext_hw_addr = smoltcp::wire::HardwareAddress::Ethernet(
			smoltcp::wire::EthernetAddress::from_bytes(&config.ext_eth_mac),
		);

		let ext_eth_iface = Enc28j60::new(
			ext_eth_spi,
			exteth_ss,
			exteth_int,
			exteth_rst,
			&mut SharedDelay,
			0x1000,
			config.ext_eth_mac,
		)
		.unwrap();

		static mut ext_eth_buf_rx: [u8; 0x1000] = [0; 0x1000];
		static mut ext_eth_buf_tx: [u8; 0x1000] = [0; 0x1000];

		let mut ext_eth_phy = enc28j60::smoltcp_phy::Phy::new(
			ext_eth_iface,
			unsafe { &mut ext_eth_buf_rx },
			unsafe { &mut ext_eth_buf_tx },
		);

		let ext_config = smoltcp::iface::Config::new(ext_hw_addr);
		// TODO ext_config.random_seed = get_random_seed()

		let ext_interface = smoltcp::iface::Interface::new(
			ext_config,
			&mut ext_eth_phy,
			smoltcp::time::Instant::from_millis(0), // TODO actually get the current time
		);

		// TODO DEBUG - needs to be implemented via trait
		exteth_xfrm_en.set_high();

		let sys_eth_spi = p.SPI1.spi(
			(syseth_sck, syseth_miso, syseth_mosi),
			MODE_0,
			10.MHz(), // TODO set to 20MHz and test (this is what the datasheet specifies)
			&clocks,
		);

		let sys_hw_addr = smoltcp::wire::HardwareAddress::Ethernet(
			smoltcp::wire::EthernetAddress::from_bytes(&config.sys_eth_mac),
		);

		let sys_eth_iface = Enc28j60::new(
			sys_eth_spi,
			syseth_ss,
			syseth_int,
			syseth_rst,
			&mut SharedDelay,
			0x1000,
			config.sys_eth_mac,
		)
		.unwrap();

		static mut sys_eth_buf_rx: [u8; 0x1000] = [0; 0x1000];
		static mut sys_eth_buf_tx: [u8; 0x1000] = [0; 0x1000];

		let mut sys_eth_phy = enc28j60::smoltcp_phy::Phy::new(
			sys_eth_iface,
			unsafe { &mut sys_eth_buf_rx },
			unsafe { &mut sys_eth_buf_tx },
		);

		let mut sys_config = smoltcp::iface::Config::new(sys_hw_addr);
		// TODO sys_config.random_seed = get_random_seed()

		let sys_interface = smoltcp::iface::Interface::new(
			sys_config,
			&mut sys_eth_phy,
			smoltcp::time::Instant::from_millis(0), // TODO actually get the current time
		);

		(
			Stm32f479DebugLed { pin: dbgled },
			Serial::tx(
				p.UART7,
				uart_tx,
				::stm32f4xx_hal::serial::Config::default()
					.baudrate(115200.bps())
					.wordlength_8()
					.stopbits(::stm32f4xx_hal::serial::config::StopBits::STOP1)
					.parity_none(),
				&clocks,
			)
			.unwrap(),
			{
				let mut indlights = Stm32f479IndicatorLights {
					en_pin: indlights_en,
					controller: super::common::is31fl3218::Is31fl3218::new(indicator_lights_iface),
				};
				indlights.controller.reset();
				indlights
			},
			Stm32f479SystemUnderTest {
				current_state: super::PowerState::Off,
				reset_pin: sys_reset,
				power_pin: sys_power,
				psu_on_pin: psu_on,
				psu_standby_pin: psu_standby,
				psu_ok_pin: psu_ok,
			},
			super::EthernetInterfaces {
				external: super::EthernetPhy {
					iface: ext_interface,
					device: ext_eth_phy,
				},
				system: super::EthernetPhy {
					iface: sys_interface,
					device: sys_eth_phy,
				},
			},
		)
	}
}

pub struct Stm32f479DebugLed {
	pin: Pin<'E', 12, Output>,
}

impl super::DebugLed for Stm32f479DebugLed {
	fn set_bit(&mut self, on: bool) {
		self.pin.set_state(PinState::from(on));
	}
}

pub struct Stm32f479IndicatorLights {
	en_pin: Pin<'B', 4, Output>,
	controller: super::common::is31fl3218::Is31fl3218<i2c::I2c<I2C1>>,
}

impl<I2C: i2c::Instance> super::common::I2c for i2c::I2c<I2C> {
	type Error = i2c::Error;
	fn read(&mut self, addr: u8, buffer: &mut [u8]) -> Result<(), Self::Error> {
		<i2c::I2c<I2C>>::read(self, addr, buffer)
	}
	fn write(&mut self, addr: u8, buffer: &[u8]) -> Result<(), Self::Error> {
		<i2c::I2c<I2C>>::write(self, addr, buffer)
	}
}

impl super::IndicatorLights for Stm32f479IndicatorLights {
	fn first<C: Into<super::Color>>(&mut self, color: C) {
		let (r, g, b) = color.into().premultiply_alpha();
		self.controller.set_channel(0, r);
		self.controller.set_channel(1, g);
		self.controller.set_channel(17, b);
		self.controller.present();
	}

	fn second<C: Into<super::Color>>(&mut self, color: C) {
		let (r, g, b) = color.into().premultiply_alpha();
		self.controller.set_channel(12, r);
		self.controller.set_channel(13, g);
		self.controller.set_channel(11, b);
		self.controller.present();
	}

	fn third<C: Into<super::Color>>(&mut self, color: C) {
		let (r, g, b) = color.into().premultiply_alpha();
		self.controller.set_channel(16, r);
		self.controller.set_channel(14, g);
		self.controller.set_channel(15, b);
		self.controller.present();
	}

	fn enable(&mut self) {
		self.en_pin.set_high();
		for _ in 0..1000 {
			unsafe { ::core::arch::asm!("NOP") };
		}
		self.controller.enable();
	}

	fn disable(&mut self) {
		self.controller.disable();
		for _ in 0..1000 {
			unsafe { ::core::arch::asm!("NOP") };
		}
		self.en_pin.set_low();
	}

	fn all_off(&mut self) {
		self.controller.reset();
	}
}

pub struct Stm32f479SystemUnderTest {
	current_state: super::PowerState,
	reset_pin: Pin<'C', 9, Output>,
	power_pin: Pin<'C', 8, Output>,
	psu_on_pin: Pin<'D', 6, Output>,
	psu_standby_pin: Pin<'D', 4, Output>,
	psu_ok_pin: Pin<'D', 5, Input>,
}

impl super::SystemUnderTest for Stm32f479SystemUnderTest {
	fn power_ok(&self) -> bool {
		self.psu_ok_pin.is_high()
	}
	fn reset_ticks(&mut self, ticks: usize) {
		self.reset_pin.set_high();
		for _ in 0..ticks {
			unsafe {
				::core::arch::asm!("NOP");
			}
		}
		self.reset_pin.set_low();
	}
	fn power_ticks(&mut self, ticks: usize) {
		self.power_pin.set_high();
		for _ in 0..ticks {
			unsafe {
				::core::arch::asm!("NOP");
			}
		}
		self.power_pin.set_low();
	}
	fn set_power_state(&mut self, new_state: super::PowerState) {
		use super::PowerState as PS;
		match (self.current_state, new_state) {
			(PS::Off, PS::Off) => { /* NO-OP */ }
			(PS::Off, PS::Standby) => {
				// Turn on the PSU standby
				self.psu_standby_pin.set_high();
				// Allow some time for the motherboard to come online
				for _ in 0..1000000 {
					unsafe {
						::core::arch::asm!("NOP");
					}
				}
			}
			(PS::Off, PS::On) => {
				// First transition to standby
				self.set_power_state(PS::Standby);
				// Then transition to on
				self.set_power_state(PS::On);
			}
			(PS::Standby, PS::Off) => {
				// Turn off the 5VSB pin
				self.psu_standby_pin.set_low();
				// Allow motherboard to drain
				for _ in 0..1000000 {
					unsafe {
						::core::arch::asm!("NOP");
					}
				}
			}
			(PS::Standby, PS::Standby) => { /* NO-OP */ }
			(PS::Standby, PS::On) => {
				// Turn on the PSU
				self.psu_on_pin.set_high();
				// Wait for the PWR_OK signal to come up.
				// roughly about 100ms
				while !self.power_ok() {}
			}
			(PS::On, PS::Off) => {
				// First transition to standby
				self.set_power_state(PS::Standby);
				// Then transition to off
				self.set_power_state(PS::Off);
			}
			(PS::On, PS::Standby) => {
				// Turn off the PSU
				self.psu_on_pin.set_low();
				// Wait for the PWR_OK signal to go low.
				// Usually around 16-150ms after (and about 1ms
				// before the rail actually go dark).
				while self.power_ok() {}
				// Give the PSU a little breathing room.
				for _ in 0..100000 {
					unsafe {
						::core::arch::asm!("NOP");
					}
				}
			}
			(PS::On, PS::On) => { /* NO-OP */ }
		}

		self.current_state = new_state;
	}
}

pub fn init_clock(rcc: &RCC) {
	// turn on HSE
	rcc.cr.write(|w| w.hseon().set_bit());
	// wait for HSE to come online
	while rcc.cr.read().hserdy().bit() {}
	// configure prescalars
	rcc.cfgr.write(|w| unsafe {
		w.ppre1()
			.bits(0b101) // APB1 /4
			.ppre2()
			.bits(0b100) // APB2 /2
			.hpre()
			.bits(0b0000) // AHB /1
		// NOTE: MCO's are not used.
	});
	// configure main PLL
	rcc.pllcfgr.write(|w| unsafe {
		w.pllm()
			.bits(22) // PLLM /22
			.plln()
			.bits(285) // PLLN X285
			.pllp()
			.bits(0b00) // PLLP /2
			.pllr()
			.bits(0b10) // PLLR /2
			.pllq()
			.bits(7) // PLLQ /7
			.pllsrc()
			.hse() // use HSE for PLL
	});
	// set PLLQ as 48Mhz clock source
	rcc.dckcfgr.write(|w| w.ck48msel().pll());
	// enable PLL
	rcc.cr.write(|w| w.pllon().set_bit());
	// wait for PLL to come online
	while rcc.cr.read().pllrdy().bit() {}
}
