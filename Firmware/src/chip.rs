#[cfg(feature = "is31fl3218")]
pub mod is31fl3218;

#[cfg(feature = "enc28j60")]
pub use embassy_net_enc28j60 as enc28j60;

use core::fmt::Debug;

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
