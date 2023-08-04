#[cfg(feature = "stm32f479vg")]
mod stm32f479vg;
#[cfg(feature = "stm32f479vg")]
pub use stm32f479vg::*;

use crate::chip;
use embassy_stm32::{
	gpio::{Input, Output, Pin},
	i2c,
};

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
> super::IndicatorLights for Is31fl3218IndicatorLights<I2C, R0, G0, B0, R1, G1, B1, R2, G2, B2>
{
	fn first<C: Into<super::Color>>(&mut self, color: C) {
		let (r, g, b) = color.into().premultiply_alpha();
		self.0.set_channel(R0, r);
		self.0.set_channel(G0, g);
		self.0.set_channel(B0, b);
		self.0.present();
	}

	fn second<C: Into<super::Color>>(&mut self, color: C) {
		let (r, g, b) = color.into().premultiply_alpha();
		self.0.set_channel(R1, r);
		self.0.set_channel(G1, g);
		self.0.set_channel(B1, b);
		self.0.present();
	}

	fn third<C: Into<super::Color>>(&mut self, color: C) {
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

pub struct DebugLed<'d, P: Pin> {
	pin: Output<'d, P>,
}

impl<'d, P: Pin> DebugLed<'d, P> {
	pub fn new(pin: Output<'d, P>) -> Self {
		Self { pin }
	}
}

impl<P: Pin> super::DebugLed for DebugLed<'_, P> {
	fn set_bit(&mut self, on: bool) {
		self.pin.set_level(on.into());
	}
}

pub struct SystemUnderTest<'d, RST, PWR, PSUON, PSUSB, SYSON>
where
	RST: Pin,
	PWR: Pin,
	PSUON: Pin,
	PSUSB: Pin,
	SYSON: Pin,
{
	current_state: super::PowerState,
	reset_pin: Output<'d, RST>,
	power_pin: Output<'d, PWR>,
	psu_on_pin: Output<'d, PSUON>,
	psu_standby_pin: Output<'d, PSUSB>,
	sys_on_pin: Input<'d, SYSON>,
}

impl<'d, RST, PWR, PSUON, PSUSB, SYSON> SystemUnderTest<'d, RST, PWR, PSUON, PSUSB, SYSON>
where
	RST: Pin,
	PWR: Pin,
	PSUON: Pin,
	PSUSB: Pin,
	SYSON: Pin,
{
	pub fn new(
		reset_pin: Output<'d, RST>,
		power_pin: Output<'d, PWR>,
		psu_on_pin: Output<'d, PSUON>,
		psu_standby_pin: Output<'d, PSUSB>,
		sys_on_pin: Input<'d, SYSON>,
	) -> Self {
		Self {
			current_state: super::PowerState::Off,
			reset_pin,
			power_pin,
			psu_on_pin,
			psu_standby_pin,
			sys_on_pin,
		}
	}
}

impl<'d, RST, PWR, PSUON, PSUSB, SYSON> super::SystemUnderTest
	for SystemUnderTest<'d, RST, PWR, PSUON, PSUSB, SYSON>
where
	RST: Pin,
	PWR: Pin,
	PSUON: Pin,
	PSUSB: Pin,
	SYSON: Pin,
{
	fn reset_ticks(&mut self, ticks: usize) {
		self.reset_pin.set_high();
		for _ in 0..ticks {
			unsafe {
				::core::arch::asm!("NOP");
			}
		}
		self.reset_pin.set_low();
	}

	fn power_ticks(&mut self, ticks: usize) {
		self.power_pin.set_high();
		for _ in 0..ticks {
			unsafe {
				::core::arch::asm!("NOP");
			}
		}
		self.power_pin.set_low();
	}

	fn current_state(&self) -> super::PowerState {
		self.current_state
	}

	fn power_requested(&self) -> bool {
		self.sys_on_pin.is_low()
	}

	unsafe fn set_power_state(&mut self, new_state: super::PowerState) {
		match new_state {
			super::PowerState::Off => {
				self.psu_on_pin.set_low();
				self.psu_standby_pin.set_low();
			}
			super::PowerState::Standby => {
				self.psu_on_pin.set_low();
				self.psu_standby_pin.set_high();
			}
			super::PowerState::On => {
				self.psu_on_pin.set_high();
				self.psu_standby_pin.set_high();
			}
		}

		self.current_state = new_state;
	}
}
