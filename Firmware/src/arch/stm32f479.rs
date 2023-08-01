use super::common::Is31fl3218;
use stm32f4xx_hal::{
	gpio::{Output, Pin, PinState},
	i2c,
	pac::{self, I2C1, RCC, UART7},
	prelude::*,
	serial::{Serial, Tx},
};

pub struct Stm32f479;

type Stm32f479DebugSerial = Tx<UART7, u8>;

impl super::Arch for Stm32f479 {
	type DebugLedImpl = Stm32f479DebugLed;
	type DebugSerialImpl = Stm32f479DebugSerial;
	type IndicatorLightsImpl = Stm32f479IndicatorLights;

	unsafe fn initialize() -> (
		Self::DebugLedImpl,
		Self::DebugSerialImpl,
		Self::IndicatorLightsImpl,
	) {
		let p = pac::Peripherals::take().unwrap();
		//let mut syscfg = p.SYSCFG.constrain();

		// Initialize the clock
		init_clock(&p.RCC);
		let clocks = p.RCC.constrain().cfgr.freeze();

		//let gpioa = p.GPIOA.split();
		let gpiob = p.GPIOB.split();
		//let gpioc = p.GPIOC.split();
		//let mut gpiod = p.GPIOD.split();
		let gpioe = p.GPIOE.split();
		//let gpiof = p.GPIOF.split();

		let indlights_scl = gpiob.pb6.into_alternate_open_drain();
		let indlights_sda = gpiob.pb7.into_alternate_open_drain();
		let indlights_en = gpiob.pb4.into_push_pull_output(); // TODO set to open-drain

		//let mut syseth_miso = gpioa.pa6.into_alternate();
		//let mut syseth_mosi = gpioa.pa7.into_alternate();
		//let mut syseth_ss = gpioa.pa4.into_alternate();
		//let mut syseth_sck = gpioa.pa5.into_alternate();
		//let mut syseth_rst = gpiob.pb1.into_push_pull_output();
		//let mut syseth_int = gpiob.pb0.make_interrupt_source(&mut syscfg);
		//let mut syseth_en = gpioa.pa2.into_push_pull_output();
		//let mut syseth_xfrm_en = gpioa.pa3.into_push_pull_output();

		//let mut oled_mosi = gpioc.pc3.into_alternate();
		//let mut oled_ss = gpiob.pb9.into_alternate();
		//let mut oled_sck = gpiod.pd3.into_alternate();
		//let mut oled_rst = gpioc.pc13.into_push_pull_output();
		//let mut oled_dc = gpioc.pc14.into_push_pull_output();
		//let mut oled_en = gpioe.pe2.into_push_pull_output();

		//let mut exteth_miso = gpioc.pc11.into_alternate();
		//let mut exteth_mosi = gpioc.pc12.into_alternate();
		//let mut exteth_ss = gpioa.pa15.into_alternate();
		//let mut exteth_sck = gpioc.pc10.into_alternate();
		//let mut exteth_rst = gpiod.pd0.into_push_pull_output();
		//let mut exteth_xfrm_en = gpiod.pd2.into_push_pull_output();
		//let mut exteth_en = gpiod.pd7.into_push_pull_output();
		//let mut exteth_int = gpiod.pd1.make_interrupt_source(&mut syscfg);

		//let mut uart_rx = gpioe.pe7.into_alternate();
		let uart_tx = gpioe.pe8.into_alternate();

		//let mut rs232_cts = gpiob.pb13.into_alternate();
		//let mut rs232_rts = gpiob.pb14.into_alternate();
		//let mut rs232_rx = gpiob.pb11.into_alternate();
		//let mut rs232_tx = gpiob.pb10.into_alternate();
		//let mut rs232_en = gpiod.pd8.into_push_pull_output();

		//let mut usb_dn = gpioa.pa11.into_alternate();
		//let mut usb_dp = gpioa.pa12.into_alternate();

		//let mut sys_power = gpioc.pc8.into_push_pull_output();
		//let mut sys_reset = gpioc.pc9.into_push_pull_output();

		//let mut psu_standby = gpiod.pd4.into_push_pull_output();
		//let mut psu_on = gpiod.pd6.into_push_pull_output();
		//let mut psu_ok = gpiod.pd5.make_interrupt_source(&mut syscfg);

		let dbgled = gpioe.pe12.into_push_pull_output();

		//self::dbg::init_dbg!(p.UART7, uart_tx, clocks);

		//oro_link_firmware::main::<Stm32F479>();

		let indicator_lights_iface = p.I2C1.i2c(
			(indlights_scl, indlights_sda),
			i2c::Mode::standard(100000.Hz()),
			&clocks,
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
			Stm32f479IndicatorLights {
				en_pin: indlights_en,
				controller: Is31fl3218::new(indicator_lights_iface),
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
	controller: Is31fl3218<i2c::I2c<I2C1>>,
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
