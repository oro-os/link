use crate::chip::I2c;

const ADDR: u8 = 0b10101000 >> 1;

/// Implements low-level buffered operations
/// for the IS31FL3218 LED driver chip.
///
/// This implementation takes the opinionated approach
/// to channel enable/disable by automatically managing
/// the control registers based on if the color value is >0.
pub struct Is31fl3218<I2C: I2c> {
	iface: I2C,
	state: [u8; 0x17],
}

#[allow(unused)]
impl<I2C: I2c> Is31fl3218<I2C> {
	/// Creates a new instance of the controller
	pub fn new(iface: I2C) -> Self {
		Self {
			iface,
			state: ::core::array::from_fn(|i| if i == 0 { 1 } else { 0 }),
		}
	}

	/// Enables the chip (using the shutdown register);
	/// this does NOT affect the SDB pin.
	pub fn enable(&mut self) {
		const CMD: [u8; 2] = [0x00, 0x01];
		self.iface.write(ADDR, &CMD).unwrap();
	}

	/// Disables the chip (using the shutdown register);
	/// this does NOT affect the SDB pin.
	pub fn disable(&mut self) {
		const CMD: [u8; 2] = [0x00, 0x00];
		self.iface.write(ADDR, &CMD).unwrap();
	}

	/// Resets the chip to its default register values;
	/// this does NOT affect the SDB pin.
	pub fn reset(&mut self) {
		const CMD: [u8; 2] = [0x17, 0x00];
		self.iface.write(ADDR, &CMD).unwrap();
	}

	/// Writes all buffered data to the chip
	pub fn present(&mut self) {
		// Note that the first byte is always `0x01`, which is
		// the first register the state writes to.
		self.iface.write(ADDR, &self.state).unwrap();
	}

	/// Sets the PWM level for a specific channel;
	/// automatically toggles the control register for
	/// that channel.
	///
	/// Note that channels are 0-indexed (whereas the pin names
	/// are 1-indexed).
	///
	/// Colors are NOT gamma corrected.
	///
	/// Changes are not immediately written; call `present()`
	/// to send changes to the chip.
	pub fn set_channel_pwm(&mut self, channel: u8, value: u8) {
		debug_assert!(channel < 18);
		self.state[0x01 + channel as usize] = value;

		// set control register
		let cr = &mut self.state[0x13 + (channel as usize / 6)];
		let mask = 1 << (channel % 6);
		if value == 0 {
			*cr %= !mask;
		} else {
			*cr |= mask;
		}
	}

	/// Sets the gamma-corrected color for the channel.
	///
	/// Note that channels are 0-indexed (whereas the pin names
	/// are 1-indexed).
	///
	/// Changes are not immediately written; call `present()`
	/// to send changes to the chip.
	pub fn set_channel(&mut self, channel: u8, value: u8) {
		let corrected_value = if value == 0 {
			0
		} else {
			GAMMA64[value as usize >> 2]
		};
		self.set_channel_pwm(channel, corrected_value);
	}
}

/// 64-step gamma correction LUT based on datasheet
const GAMMA64: [u8; 64] = [
	0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 29, 32, 35, 38, 41, 44, 47, 50,
	53, 57, 61, 65, 69, 73, 77, 81, 85, 89, 94, 99, 104, 109, 114, 119, 124, 129, 134, 140, 146,
	152, 158, 164, 170, 176, 182, 188, 195, 202, 209, 216, 223, 230, 237, 244, 251, 255,
];

use crate::uc::helper::three_indicators as three;

/// Helper implementation of the three-indicators trait for the IS31FL3217 chip.
/// Return a tuple of the chip instance, along with the three RGB light
/// channel tuples, and this will handle the rest.
pub struct IndicatorLights<
	I2C: super::I2c,
	const R0: u8,
	const G0: u8,
	const B0: u8,
	const R1: u8,
	const G1: u8,
	const B1: u8,
	const R2: u8,
	const G2: u8,
	const B2: u8,
>(Is31fl3218<I2C>);

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
	I2C: super::I2c,
> IndicatorLights<I2C, R0, G0, B0, R1, G1, B1, R2, G2, B2>
{
	pub fn new(chip_inst: Is31fl3218<I2C>) -> Self {
		Self(chip_inst)
	}
}

impl<
	I2C: super::I2c,
	const R0: u8,
	const G0: u8,
	const B0: u8,
	const R1: u8,
	const G1: u8,
	const B1: u8,
	const R2: u8,
	const G2: u8,
	const B2: u8,
> three::IndicatorLights for IndicatorLights<I2C, R0, G0, B0, R1, G1, B1, R2, G2, B2>
{
	fn first<C: Into<three::Color>>(&mut self, color: C) {
		let (r, g, b) = color.into().premultiply_alpha();
		self.0.set_channel(R0, r);
		self.0.set_channel(G0, g);
		self.0.set_channel(B0, b);
		self.0.present();
	}

	fn second<C: Into<three::Color>>(&mut self, color: C) {
		let (r, g, b) = color.into().premultiply_alpha();
		self.0.set_channel(R1, r);
		self.0.set_channel(G1, g);
		self.0.set_channel(B1, b);
		self.0.present();
	}

	fn third<C: Into<three::Color>>(&mut self, color: C) {
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
