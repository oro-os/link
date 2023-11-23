#[cfg(feature = "is31fl3218")]
pub mod is31fl3218;

#[cfg(feature = "ssd1362")]
pub mod ssd1362;

#[cfg(feature = "enc28j60")]
pub mod enc28j60;

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

/// SPI proxy abstraction; implementations must handle switching/slave select
/// as well as mode selection, etc.
pub trait Transact {
	/// The opaque error type for read/write errors by the underlying
	/// HAL subsystem.
	type Error: Debug;

	/// Performs a transaction
	///
	/// Panics if `tx.len() != rx.len()`.
	fn transact(&mut self, tx: &[u8], rx: &mut [u8]) -> Result<(), Self::Error>;
}

/// SPI proxy abstraction; implementations must handle switching/slave select
/// as well as mode selection, etc.
pub trait TransactRead {
	/// The opaque error type for read/write errors by the underlying
	/// HAL subsystem.
	type Error: Debug;

	/// Performs a transaction (read-only)
	fn transact_ro(&mut self, rx: &mut [u8]) -> Result<(), Self::Error>;
}

/// SPI proxy abstraction; implementations must handle switching/slave select
/// as well as mode selection, etc.
pub trait TransactWrite {
	/// The opaque error type for read/write errors by the underlying
	/// HAL subsystem.
	type Error: Debug;

	/// Performs a transaction (write-only)
	fn transact_wo(&mut self, tx: &[u8]) -> Result<(), Self::Error>;
}

#[cfg(feature = "embedded-hal")]
impl<SPI: embedded_hal::spi::SpiDevice<u8>> Transact for SPI {
	type Error = SPI::Error;

	fn transact(&mut self, tx: &[u8], rx: &mut [u8]) -> Result<(), Self::Error> {
		debug_assert_eq!(tx.len(), rx.len());
		self.transfer(rx, tx)
	}
}

#[cfg(feature = "embedded-hal")]
impl<SPI: embedded_hal::spi::SpiDevice<u8>> TransactRead for SPI {
	type Error = SPI::Error;

	fn transact_ro(&mut self, rx: &mut [u8]) -> Result<(), Self::Error> {
		self.read(rx)
	}
}

#[cfg(feature = "embedded-hal")]
impl<SPI: embedded_hal::spi::SpiDevice<u8>> TransactWrite for SPI {
	type Error = SPI::Error;

	fn transact_wo(&mut self, tx: &[u8]) -> Result<(), Self::Error> {
		self.write(tx)
	}
}

/// A proxy for a GPIO Pin
pub trait Pin {
	// Set the pin high
	fn high(&mut self);
	// Set the pin low
	fn low(&mut self);
}

#[cfg(feature = "embedded-hal")]
impl<T: embedded_hal::digital::OutputPin> Pin for T {
	fn high(&mut self) {
		self.set_high().unwrap();
	}
	fn low(&mut self) {
		self.set_low().unwrap();
	}
}

/// A buffered draw target thus needing a call to `.present()` when the frame should be
/// pushed to the peripheral.
#[allow(unused)]
pub trait BufferedDrawTarget {
	type Error: Debug;

	/// Push the buffered pixel data to the peripheral.
	fn present(&mut self) -> Result<(), Self::Error>;
}

/// A power state of the peripheral.
/// For states that cannot be represented by a peripheral, use `.simplified()`.
///
/// For users of this enum, you should never use `Invert` and `Dim` together.
/// Only use `Invert` and `On`.
#[allow(unused)]
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum OledPowerState {
	/// Peripheral should be immediately put into standby mode.
	Standby,
	/// Peripheral should be turned on to full brightness, non-inverted.
	On,
	/// Peripheral should be set to a dimmed state (a reasonably low-light
	/// but still readable brightness), non-inverted.
	Dimmed,
	/// Peripheral should fade out after ~30 seconds and subsequently go into
	/// standby mode, non-inverted.
	FadeOut,
	/// Peripheral should invert the display. If not possible, call `.no_invert()`
	/// and use the result (which is guaranteed not to return `Invert`).
	Invert,
}

impl OledPowerState {
	/// For peripherals that cannot handle inverting the display, call this function
	/// and handle the result normally.
	#[allow(unused)]
	fn no_invert(self) -> Self {
		match self {
			Self::Invert => Self::Dimmed,
			_ => self,
		}
	}

	/// For peripherals that cannot handle nuanced power/brightness states, use this
	/// method to get an "on" or "off" boolean.
	#[allow(unused)]
	fn simplified(self) -> bool {
		match self {
			Self::Standby => false,
			Self::On => true,
			Self::Dimmed => true,
			Self::FadeOut => false,
			Self::Invert => false,
		}
	}
}

/// An OLED peripheral (or something like it) that with a managed power state, etc.
#[allow(unused)]
pub trait OledPeripheral {
	/// Sets the power state of the OLED peripheral.
	fn set_power_state(&mut self, state: OledPowerState);
}
