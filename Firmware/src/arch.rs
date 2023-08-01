mod common;
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
	/// Concrete implementation for the indicator lights
	type IndicatorLightsImpl: IndicatorLights;

	/// Initializes the Oro Link architecture and all of the necessary peripherals
	/// needed for operation
	///
	/// # Safety
	///
	/// Must only be called **ONCE** during the initialization of the firmware!
	///
	/// Further, **this function may NOT use `println!()` or any other print functions!**
	/// It should also avoid panicking. No output will show up!
	unsafe fn initialize() -> (
		Self::DebugLedImpl,
		Self::DebugSerialImpl,
		Self::IndicatorLightsImpl,
	);
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

pub mod color {
	use super::Color;

	pub const BLACK: Color = Color::new_rgba(0);
	pub const WHITE: Color = Color::new_rgba(0xFFFFFFFF);
	pub const RED: Color = Color::new_rgba(0xFF0000FF);
	pub const GREEN: Color = Color::new_rgba(0x00FF00FF);
	pub const BLUE: Color = Color::new_rgba(0xFF0000FF);
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
