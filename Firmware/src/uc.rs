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
	/// with a pulse length of 50ms.
	fn reset(&mut self) {
		self.reset_ms(50);
	}

	/// Triggers a reset of the machine (via the reset switch),
	/// holding it for a specific number of milliseconds.
	fn reset_ms(&mut self, ms: u64);

	/// Triggers an ACPI power signal to the machine (via the power switch)
	/// with a pulse length of 50ms.
	fn power(&mut self) {
		self.power_ms(50);
	}

	/// Triggers an ACPI power signal to the machine (via the power switch),
	/// holding it for a specific number of milliseconds.
	fn power_ms(&mut self, ms: u64);
}

/// A singular color of an indicator light; may be gamma corrected
/// by the implementation (do not gamma correct yourself).
#[derive(Debug, Clone, Copy)]
pub struct Color {
	pub r: u8,
	pub g: u8,
	pub b: u8,
	pub a: u8,
}

#[allow(unused)]
impl Color {
	/// Constructs a new `Color` given individual component values
	pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
		Self { r, g, b, a }
	}

	/// Constructs a new `Color` given a single RGBA integer
	/// (e.g. 0xAABBCCDD)
	pub const fn new_rgba(rgba: u32) -> Self {
		Self {
			r: ((rgba >> 24) & 0xFF) as u8,
			g: ((rgba >> 16) & 0xFF) as u8,
			b: ((rgba >> 8) & 0xFF) as u8,
			a: (rgba & 0xFF) as u8,
		}
	}

	/// Modifies the alpha value
	pub const fn alpha(mut self, new_alpha: u8) -> Self {
		self.a = new_alpha;
		self
	}

	/// Modifies the red value
	pub const fn red(mut self, new_red: u8) -> Self {
		self.r = new_red;
		self
	}

	/// Modifies the blue value
	pub const fn blue(mut self, new_blue: u8) -> Self {
		self.b = new_blue;
		self
	}

	/// Modifies the green value
	pub const fn green(mut self, new_green: u8) -> Self {
		self.g = new_green;
		self
	}

	/// Creates a new color with the modified alpha value
	pub const fn with_alpha(&self, new_alpha: u8) -> Self {
		Self {
			r: self.r,
			g: self.g,
			b: self.b,
			a: new_alpha,
		}
	}

	/// Creates a new color with the modified red value
	pub const fn with_red(&self, new_red: u8) -> Self {
		Self {
			r: new_red,
			g: self.g,
			b: self.b,
			a: self.a,
		}
	}

	/// Creates a new color with the modified blue value
	pub const fn with_blue(&self, new_blue: u8) -> Self {
		Self {
			r: self.r,
			g: self.g,
			b: new_blue,
			a: self.a,
		}
	}

	/// Creates a new color with the modified green value
	pub const fn with_green(&self, new_green: u8) -> Self {
		Self {
			r: self.r,
			g: new_green,
			b: self.b,
			a: self.a,
		}
	}

	/// Pre-multiplies the alpha without performing a float cast
	pub fn premultiply_alpha(&self) -> (u8, u8, u8) {
		(
			((((self.r as u16) * (self.a as u16)) >> 8) + 1) as u8,
			((((self.g as u16) * (self.a as u16)) >> 8) + 1) as u8,
			((((self.b as u16) * (self.a as u16)) >> 8) + 1) as u8,
		)
	}
}

impl From<u32> for Color {
	fn from(v: u32) -> Self {
		Color::new_rgba(v)
	}
}

impl From<(u8, u8, u8, u8)> for Color {
	fn from(v: (u8, u8, u8, u8)) -> Self {
		Color::new(v.0, v.1, v.2, v.3)
	}
}

#[allow(unused)]
pub mod color {
	use super::Color;

	pub const BLACK: Color = Color::new_rgba(0);
	pub const WHITE: Color = Color::new_rgba(0xFFFFFFFF);
	pub const RED: Color = Color::new_rgba(0xFF0000FF);
	pub const GREEN: Color = Color::new_rgba(0x00FF00FF);
	pub const BLUE: Color = Color::new_rgba(0x0000FFFF);
	pub const CYAN: Color = Color::new_rgba(0x00FFFFFF);
	pub const MAGENTA: Color = Color::new_rgba(0xFF00FFFF);
	pub const YELLOW: Color = Color::new_rgba(0xFFFF00FF);
}

/// Controller for the 3 indicator lights
pub trait IndicatorLights {
	/// Sets the color of the first indicator light
	fn first<C: Into<Color>>(&mut self, color: C);

	/// Sets the color of the second indicator light
	fn second<C: Into<Color>>(&mut self, color: C);

	/// Sets the color of the third indicator light
	fn third<C: Into<Color>>(&mut self, color: C);

	/// Turns off all lights (may not disable the chip)
	fn all_off(&mut self) {
		self.first(color::BLACK);
		self.second(color::BLACK);
		self.third(color::BLACK);
	}

	/// Disables the controller (may be a no-op on unsupported chips)
	fn disable(&mut self) {}

	/// Enables the controller (may be a no-op on unsupported chips)
	fn enable(&mut self) {}
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

	impl<DBG: DebugLed, SUT: SystemUnderTest, IND: IndicatorLights> Init for (DBG, SUT, IND) {}
}
