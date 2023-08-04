use crate::uc;
use embassy_stm32::{
	bind_interrupts,
	dma::NoDma,
	gpio::{Level, Output, Speed},
	i2c::{self, I2c},
	peripherals,
	time::Hertz,
	Config,
};

bind_interrupts!(struct Irqs {
	I2C1_EV => i2c::InterruptHandler<peripherals::I2C1>;
});

pub fn init() -> (impl uc::DebugLed, ()) {
	let config = Config::default();

	let p = embassy_stm32::init(config);

	let led = Output::new(p.PE12, Level::Low, Speed::Low);

	let mut ind_on = Output::new(p.PB4, Level::Low, Speed::Low);

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

	ind_on.set_high();
	let mut indicators = crate::chip::is31fl3218::Is31fl3218::new(i2c);
	indicators.enable();

	(peripheral::DebugLed::new(led), ())
}

pub mod peripheral {
	use crate::uc;
	use embassy_stm32::gpio::{Output, Pin};

	pub struct DebugLed<'d, P: Pin> {
		pin: Output<'d, P>,
	}

	impl<'d, P: Pin> DebugLed<'d, P> {
		pub fn new(pin: Output<'d, P>) -> Self {
			Self { pin }
		}
	}

	impl<P: Pin> uc::DebugLed for DebugLed<'_, P> {
		fn set_bit(&mut self, on: bool) {
			self.pin.set_level(on.into());
		}
	}
}
