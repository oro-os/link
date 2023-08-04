#[cfg(feature = "stm32")]
mod stm32;
#[cfg(feature = "stm32")]
pub use stm32::*;

use embassy_time::{block_for, Duration};

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

/// Power states in which a system under test can be.
/// The controller will properly transition between them,
/// which may block for a few hundred milliseconds.
#[allow(unused)]
#[derive(Debug, Clone, Copy)]
pub enum PowerState {
	/// Both the motherboard +5VSB line and PSU are off.
	Off,
	/// The motherboard +5bSB line is on, but the PSU is off.
	Standby,
	/// Both the motherboard +5VSB line and PSU are on.
	On,
}

/// Controller for the system under test (switches, CPU, etc.)
pub trait SystemUnderTest {
	/// Returns whether or not the SUT is requesting for the PSU
	/// to be turned on.
	fn power_requested(&self) -> bool;

	/// Returns the current power state of the SUT.
	fn current_state(&self) -> PowerState;

	/// Set the power state of the machine. Must transition between
	/// all combinations properly. This function MIGHT BLOCK the entire system.
	fn transition_power_state(&mut self, new_state: PowerState) {
		use PowerState as PS;

		match (self.current_state(), new_state) {
			(PS::Off, PS::Off) => { /* NO-OP */ }
			(PS::Off, PS::Standby) => {
				// Turn on the PSU standby
				unsafe {
					self.set_power_state(PS::Standby);
				}
				// Allow some time for the motherboard to come online
				block_for(Duration::from_millis(50));
			}
			(PS::Off, PS::On) => {
				// First transition to standby
				self.transition_power_state(PS::Standby);
				// Then transition to on
				self.transition_power_state(PS::On);
			}
			(PS::Standby, PS::Off) => {
				// Turn off the 5VSB pin
				unsafe {
					self.set_power_state(PS::Off);
				}
				// Allow motherboard to drain
				// ATX standard dictates no less than 16ms.
				block_for(Duration::from_millis(50));
			}
			(PS::Standby, PS::Standby) => { /* NO-OP */ }
			(PS::Standby, PS::On) => {
				// Turn on the PSU
				unsafe {
					self.set_power_state(PS::On);
				}
				// Wait for power to become stable
				// ATX standard dictates no less than 100ms.
				block_for(Duration::from_millis(150));
			}
			(PS::On, PS::Off) => {
				// First transition to standby
				self.transition_power_state(PS::Standby);
				// Then transition to off
				self.transition_power_state(PS::Off);
			}
			(PS::On, PS::Standby) => {
				// Turn off the PSU
				unsafe {
					self.set_power_state(PS::Standby);
				}
				// Give the PSU a little breathing room.
				block_for(Duration::from_millis(50));
			}
			(PS::On, PS::On) => { /* NO-OP */ }
		}
	}

	/// Immediately set the power state of the machine.
	///
	/// # Safety
	///
	/// No transitioning is done; calling this function too quickly
	/// with different states *may* cause system instability or, perhaps,
	/// even damage in the extreme edge case. Use it cautiously, and with
	/// sufficient delays in between.
	///
	/// You should probably use `transition_power_state` instead.
	unsafe fn set_power_state(&mut self, new_state: PowerState);

	/// Triggers a reset of the machine (via the reset switch)
	/// with a pulse length of 10000 NOP's.
	fn reset(&mut self) {
		self.reset_ticks(100000);
	}

	/// Triggers a reset of the machine (via the reset switch)
	/// with the specify number of NOP instructions (ticks).
	fn reset_ticks(&mut self, ticks: usize);

	/// Triggers an ACPI power signal to the machine (via the power switch)
	/// with a pulse length of 10000 NOP's.
	fn power(&mut self) {
		self.reset_ticks(100000);
	}

	/// Triggers an ACPI power signal to the machine (via the power switch)
	/// with the specify number of NOP instructions (ticks).
	fn power_ticks(&mut self, ticks: usize);
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

	impl<DBG: DebugLed, SUT: SystemUnderTest> Init for (DBG, SUT) {}
}
