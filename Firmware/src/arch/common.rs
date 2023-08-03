//! Common implementations of peripherals based on their chips,
//! feature-gated based on the platform.

use core::fmt::Debug;

// Indicator lighting controller: IS31FL3218
#[cfg(feature = "is31fl3218")]
pub mod is31fl3218;

/// I2C proxy abstraction; since each of the archs
/// will use different HAL backend libraries of their own,
/// each implementing I2C with their own types, there needs
/// to be a common type between them that the common peripheral
/// types can use to accept and interact with the concrete
/// implementations of the underlying types.
pub trait I2c {
	/// The opaque error type for read/write errors by the underlying
	/// HAL subsystem.
	type Error: Debug;

	/// Read a buffer from the I2C device
	fn read(&mut self, addr: u8, buffer: &mut [u8]) -> Result<(), Self::Error>;
	/// Write a buffer to the I2C device
	fn write(&mut self, addr: u8, buffer: &[u8]) -> Result<(), Self::Error>;
}
