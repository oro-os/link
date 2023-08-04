use crate::uc;
use embassy_stm32::{
	bind_interrupts,
	dma::NoDma,
	gpio::{Input, Level, Output, Pull, Speed},
	i2c::{self, I2c},
	peripherals,
	time::Hertz,
	Config,
};

bind_interrupts!(struct Irqs {
	I2C1_EV => i2c::InterruptHandler<peripherals::I2C1>;
});

pub fn init() -> (impl uc::DebugLed, impl uc::SystemUnderTest) {
	let config = Config::default();

	let p = embassy_stm32::init(config);

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

	(
		peripheral::DebugLed::new(Output::new(p.PE12, Level::Low, Speed::Low)),
		peripheral::SystemUnderTest::new(
			Output::new(p.PC9, Level::Low, Speed::Low),
			Output::new(p.PC8, Level::Low, Speed::Low),
			Output::new(p.PD6, Level::Low, Speed::Low),
			Output::new(p.PD4, Level::Low, Speed::Low),
			Input::new(p.PD5, Pull::Up),
		),
	)
}

pub mod peripheral {
	use crate::uc;
	use embassy_stm32::gpio::{Input, Output, Pin};

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

	pub struct SystemUnderTest<'d, RST, PWR, PSUON, PSUSB, SYSON>
	where
		RST: Pin,
		PWR: Pin,
		PSUON: Pin,
		PSUSB: Pin,
		SYSON: Pin,
	{
		current_state: uc::PowerState,
		reset_pin: Output<'d, RST>,
		power_pin: Output<'d, PWR>,
		psu_on_pin: Output<'d, PSUON>,
		psu_standby_pin: Output<'d, PSUSB>,
		sys_on_pin: Input<'d, SYSON>,
	}

	impl<'d, RST, PWR, PSUON, PSUSB, SYSON> SystemUnderTest<'d, RST, PWR, PSUON, PSUSB, SYSON>
	where
		RST: Pin,
		PWR: Pin,
		PSUON: Pin,
		PSUSB: Pin,
		SYSON: Pin,
	{
		pub fn new(
			reset_pin: Output<'d, RST>,
			power_pin: Output<'d, PWR>,
			psu_on_pin: Output<'d, PSUON>,
			psu_standby_pin: Output<'d, PSUSB>,
			sys_on_pin: Input<'d, SYSON>,
		) -> Self {
			Self {
				current_state: uc::PowerState::Off,
				reset_pin,
				power_pin,
				psu_on_pin,
				psu_standby_pin,
				sys_on_pin,
			}
		}
	}

	impl<'d, RST, PWR, PSUON, PSUSB, SYSON> uc::SystemUnderTest
		for SystemUnderTest<'d, RST, PWR, PSUON, PSUSB, SYSON>
	where
		RST: Pin,
		PWR: Pin,
		PSUON: Pin,
		PSUSB: Pin,
		SYSON: Pin,
	{
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

		fn current_state(&self) -> uc::PowerState {
			self.current_state
		}

		fn power_requested(&self) -> bool {
			self.sys_on_pin.is_low()
		}

		unsafe fn set_power_state(&mut self, new_state: uc::PowerState) {
			match new_state {
				uc::PowerState::Off => {
					self.psu_on_pin.set_low();
					self.psu_standby_pin.set_low();
				}
				uc::PowerState::Standby => {
					self.psu_on_pin.set_low();
					self.psu_standby_pin.set_high();
				}
				uc::PowerState::On => {
					self.psu_on_pin.set_high();
					self.psu_standby_pin.set_high();
				}
			}

			self.current_state = new_state;
		}
	}
}
