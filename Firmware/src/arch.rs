mod common;
#[cfg(feature = "stm32f479")]
mod stm32f479;

#[cfg(feature = "stm32f479")]
pub use stm32f479::Stm32f479 as Impl;

use core::fmt::Write;
use smoltcp::{iface::Interface as EthernetInterface, phy::Device as PhyDevice};

// Assert that the implementation implements Arch
#[doc(hidden)]
#[allow(unused)]
fn implements_arch()
where
	Impl: Arch,
{
}

/// Configuration for the architecture's peripherals
pub struct ArchConfig {
	pub ext_eth_mac: [u8; 6],
	pub sys_eth_mac: [u8; 6],
}

/// Owning handles to an ethernet phy and its device
pub struct EthernetPhy<PHY: PhyDevice> {
	/// The smoltcp ethernet interface
	pub iface: EthernetInterface,
	/// The phy device
	pub device: PHY,
}

/// Set of ethernet interfaces corresponding to the external/system interfaces
pub struct EthernetInterfaces<EXT: PhyDevice, SYS: PhyDevice> {
	/// The network interface that is connected to the outside world
	pub external: EthernetPhy<EXT>,
	/// The network interface that is connected to the system under test
	pub system: EthernetPhy<SYS>,
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
	/// Concrete implementation for the system-under-test (SUT) controller
	type SystemUnderTestImpl: SystemUnderTest;
	/// Concrete implementation for the external ethernet PHY device
	type ExternalEthernetDeviceImpl: PhyDevice;
	/// Concrete implementation for the system ethernet PHY device
	type SystemEthernetDeviceImpl: PhyDevice;

	/// Initializes the Oro Link architecture and all of the necessary peripherals
	/// needed for operation
	///
	/// # Safety
	///
	/// Must only be called **ONCE** during the initialization of the firmware!
	///
	/// Further, **this function may NOT use `println!()` or any other print functions!**
	/// It should also avoid panicking. No output will show up!
	unsafe fn initialize(
		config: ArchConfig,
	) -> (
		Self::DebugLedImpl,
		Self::DebugSerialImpl,
		Self::IndicatorLightsImpl,
		Self::SystemUnderTestImpl,
		EthernetInterfaces<Self::ExternalEthernetDeviceImpl, Self::SystemEthernetDeviceImpl>,
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

/// Power states in which a system under test can be.
/// The controller will properly transition between them,
/// which may block for a few hundred milliseconds.
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
	/// Set the power state of the machine. Must transition between
	/// all combinations properly, which **might block** if the operation
	/// is complex.
	fn set_power_state(&mut self, new_state: PowerState);

	/// Triggers a reset of the machine (via the reset switch)
	/// with a pulse length of 10000 NOP's.
	fn reset(&mut self) {
		self.reset_ticks(10000);
	}

	/// Triggers a reset of the machine (via the reset switch)
	/// with the specify number of NOP instructions (ticks).
	fn reset_ticks(&mut self, ticks: usize);

	/// Triggers an ACPI power signal to the machine (via the power switch)
	/// with a pulse length of 10000 NOP's.
	fn power(&mut self) {
		self.reset_ticks(10000);
	}

	/// Triggers an ACPI power signal to the machine (via the power switch)
	/// with the specify number of NOP instructions (ticks).
	fn power_ticks(&mut self, ticks: usize);

	/// Returns whether or not PWR_OK is high.
	fn power_ok(&self) -> bool;
}
