#[cfg(feature = "stm32")]
mod stm32;
#[cfg(feature = "stm32")]
pub use stm32::*;

/// Controller for the MCU's debug LED, which is just a single LED used
/// to test basic I/O during POST and other states the firmware decides
/// to use it for.
pub trait DebugLed {
	/// Sets the LED on or off
	fn set_bit(&mut self, on: bool);

	/// Turns the LED on
	fn on(&mut self) {
		self.set_bit(true);
	}

	/// Turns the LED off
	fn off(&mut self) {
		self.set_bit(false);
	}
}

// Validates the contract of the init() function.
#[allow(unused)]
#[doc(hidden)]
mod _check_init {
	use super::*;
	trait Init {
		fn ok(self)
		where
			Self: Sized,
		{
		}
	}

	fn _check_init() {
		Init::ok(init())
	}

	impl<DBG: DebugLed> Init for (DBG, ()) {}
}
