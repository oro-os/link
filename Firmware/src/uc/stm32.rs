#[cfg(feature = "stm32f479vg")]
mod stm32f479vg;
#[cfg(feature = "stm32f479vg")]
pub use stm32f479vg::*;

use crate::chip;
use embassy_stm32::{
	gpio::{Input, Output, Pin},
	i2c,
};
use embassy_time::{block_for, Duration};

/// Implementation of I2c proxies for STM32 I2c peripherals.
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

/// Implements a (`DebugLed`)(super::DebugLed) for a single STM32 pin.
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

/// Implements a (`SystemUnderTest`)[super::SystemUnderTest] for a collection of STM32 pins.
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
	fn reset_ms(&mut self, ms: u64) {
		self.reset_pin.set_high();
		block_for(Duration::from_millis(ms));
		self.reset_pin.set_low();
	}

	fn power_ms(&mut self, ms: u64) {
		self.power_pin.set_high();
		block_for(Duration::from_millis(ms));
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
