#[cfg(feature = "stm32f479vg")]
mod stm32f479vg;
#[cfg(feature = "stm32f479vg")]
pub use stm32f479vg::*;

use crate::{chip, uc::Color};
use embassy_stm32::i2c;

impl<'d, T: i2c::Instance, TXDMA, RXDMA> chip::I2c for i2c::I2c<'d, T, TXDMA, RXDMA> {
	type Error = i2c::Error;

	#[inline]
	fn read(&mut self, addr: u8, buffer: &mut [u8]) -> Result<(), Self::Error> {
		i2c::I2c::blocking_read(self, addr, buffer)
	}

	#[inline]
	fn write(&mut self, addr: u8, buffer: &[u8]) -> Result<(), Self::Error> {
		i2c::I2c::blocking_write(self, addr, buffer)
	}
}

/// Helper implementation for the IS31FL3217 chip.
/// Return a tuple of the chip instance, along with the three RGB light
/// channel tuples, and this will handle the rest.
#[cfg(feature = "is31fl3218")]
pub struct Is31fl3218IndicatorLights<
	I2C: chip::I2c,
	const R0: u8,
	const G0: u8,
	const B0: u8,
	const R1: u8,
	const G1: u8,
	const B1: u8,
	const R2: u8,
	const G2: u8,
	const B2: u8,
>(crate::chip::is31fl3218::Is31fl3218<I2C>);

#[allow(unused)]
impl<
	const R0: u8,
	const G0: u8,
	const B0: u8,
	const R1: u8,
	const G1: u8,
	const B1: u8,
	const R2: u8,
	const G2: u8,
	const B2: u8,
	I2C: chip::I2c,
> Is31fl3218IndicatorLights<I2C, R0, G0, B0, R1, G1, B1, R2, G2, B2>
{
	pub fn new(chip_inst: crate::chip::is31fl3218::Is31fl3218<I2C>) -> Self {
		Self(chip_inst)
	}
}

impl<
	I2C: chip::I2c,
	const R0: u8,
	const G0: u8,
	const B0: u8,
	const R1: u8,
	const G1: u8,
	const B1: u8,
	const R2: u8,
	const G2: u8,
	const B2: u8,
> crate::uc::IndicatorLights
	for Is31fl3218IndicatorLights<I2C, R0, G0, B0, R1, G1, B1, R2, G2, B2>
{
	fn first<C: Into<Color>>(&mut self, color: C) {
		let (r, g, b) = color.into().premultiply_alpha();
		self.0.set_channel(R0, r);
		self.0.set_channel(G0, g);
		self.0.set_channel(B0, b);
		self.0.present();
	}

	fn second<C: Into<Color>>(&mut self, color: C) {
		let (r, g, b) = color.into().premultiply_alpha();
		self.0.set_channel(R1, r);
		self.0.set_channel(G1, g);
		self.0.set_channel(B1, b);
		self.0.present();
	}

	fn third<C: Into<Color>>(&mut self, color: C) {
		let (r, g, b) = color.into().premultiply_alpha();
		self.0.set_channel(R2, r);
		self.0.set_channel(G2, g);
		self.0.set_channel(B2, b);
		self.0.present();
	}

	fn all_off(&mut self) {
		self.0.reset();
	}

	fn disable(&mut self) {
		self.0.disable();
	}

	fn enable(&mut self) {
		self.0.enable();
	}
}
