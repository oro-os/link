#[cfg(feature = "stm32f479")]
mod stm32f479;

#[cfg(feature = "stm32f479")]
pub use stm32f479::Stm32f479 as Impl;

use core::fmt::Write;

// Assert that the implementation implements Arch
#[doc(hidden)]
#[allow(unused)]
fn implements_arch()
where
	Impl: Arch,
{
}

/// Any supported "architecture" (platform/chip/...) must implement this
/// trait, which exposes a set of implementation types and a singular,
/// call-once function `initialize()`.
pub trait Arch {
	/// Concrete implementation for the debug LED controller
	type DebugLedImpl: DebugLed;
	/// Concrete implementation for debug output serial line
	/// (NOT the USART that controls the SUT's RS232 port)
	type DebugSerialImpl: Write;

	/// Initializes the Oro Link architecture and all of the necessary peripherals
	/// needed for operation
	///
	/// # Safety
	///
	/// Must only be called **ONCE** during the initialization of the firmware!
	unsafe fn initialize() -> (Self::DebugLedImpl, Self::DebugSerialImpl);
}

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
