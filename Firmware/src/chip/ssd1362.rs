//! SSD1362-based OLED screens
//! Requires SPI + D/C line ("4-wire SPI")

use embedded_graphics::{
	framebuffer::{buffer_size, Framebuffer},
	pixelcolor::{raw::LittleEndian, Gray4, PixelColor},
	prelude::*,
};
use embedded_graphics_core::{
	draw_target::DrawTarget,
	geometry::{OriginDimensions, Size},
	primitives::Rectangle,
};

type FrameBuf = Framebuffer<
	Gray4,
	<Gray4 as PixelColor>::Raw,
	LittleEndian,
	256,
	64,
	{ buffer_size::<Gray4>(256, 64) },
>;

/// Implements low-level communication for SSD1362-based
/// OLED/LCD/LED screens.
pub struct SSD1362<SPI: super::TransactWrite, DC: super::Pin> {
	spi: SPI,
	dc: DC,
	framebuf: FrameBuf,
}

macro_rules! cmd {
	($self:expr) => {
		$self.dc.low()
	};
}

macro_rules! data {
	($self:expr) => {
		$self.dc.high()
	};
}

macro_rules! send {
	($self:expr, [ $($bytes:expr),+ $(,)? ]) => {{
		let buf = [$($bytes),+];
		$self.spi.transact_wo(&buf[..])
	}}
}

macro_rules! reset_cursor {
	($self:expr) => {{
		send!(
			$self,
			[
				// Set column address...
				0x15, // ... starting at column 0
				0,    // ... and ending at column (SEG) 127
				127,  // Set row address...
				0x75, // ... starting at row 0
				0,    // ... and ending at row 63
				63
			]
		)
	}};
}

impl<SPI: super::TransactWrite, DC: super::Pin> SSD1362<SPI, DC> {
	/// Gamma value for each of the 16 luminance values `x = 0..15`
	/// is calculated as `(gs/15)((x + 1)/64) * 180` (180 being max).
	/// Thus, a value of `64` is linear gamma, higher values
	/// bias low (slow-to-bright) and lower values bias
	/// high (fast-to-bright).
	///
	/// Calculator: https://www.desmos.com/calculator/clxn0szg2p
	pub fn new(spi: SPI, dc: DC, flip: bool, gamma: u8) -> Result<Self, SPI::Error> {
		let mut s = Self {
			spi,
			dc,
			framebuf: FrameBuf::new(),
		};
		s.init(flip, gamma)?;
		s.on()?;
		s.clear()?;
		Ok(s)
	}

	fn init(&mut self, flip: bool, gamma: u8) -> Result<(), SPI::Error> {
		// Calculate gamma steps
		let mut gamma_cmd = [0u8; 15];
		gamma_cmd[0] = 0xB8; // Set gamma table
		let a: f32 = gamma.into();
		let a = (a + 1.0) / 64.0;
		for i in 1..16 {
			use micromath::F32Ext;
			let x: f32 = i as f32;
			let gv = (x / 15.0).powf(a) * 180.0;
			gamma_cmd[i - 1] = gv as u8;
		}

		cmd!(self);
		send!(
			self,
			[
				// Set Command Lock
				0xFD, // (12H=Unlock,16H=Lock)
				0x12,
			]
		)?;
		send!(
			self,
			[
				// Display OFF(Sleep Mode)
				0xAE,
			]
		)?;
		send!(
			self,
			[
				// Set column address
				0x15, // Start column address
				0x00, // End column Address
				0x7F,
			]
		)?;
		send!(
			self,
			[
				// Set Row Address
				0x75, // Start Row Address
				0x00, // End Row Address
				0x3F,
			]
		)?;
		send!(
			self,
			[
				// Set contrast
				0x81, 0xFF,
			]
		)?;
		send!(
			self,
			[
				// Set Remap
				0xA0,
				if flip { 0b01010010 } else { 0b11000011 },
			]
		)?;
		send!(
			self,
			[
				// Set Display Start Line
				0xA1, 0x00,
			]
		)?;
		send!(
			self,
			[
				// Set Display Offset
				0xA2, 0x00,
			]
		)?;
		send!(
			self,
			[
				// Normal Display
				0xA4,
			]
		)?;
		send!(
			self,
			[
				// Set Multiplex Ratio
				0xA8, 0x3F,
			]
		)?;
		send!(
			self,
			[
				// Set VDD regulator
				0xAB,
			]
		)?;
		send!(
			self,
			[
				// Regulator Enable
				0x01,
			]
		)?;
		send!(
			self,
			[
				// External /Internal IREF Selection
				0b10011110, 0x8E,
			]
		)?;
		send!(
			self,
			[
				// Set Phase Length
				0xB1, 0x22,
			]
		)?;
		send!(
			self,
			[
				// Display clock Divider
				0xB3,
				#[allow(clippy::identity_op)]
				(
					// Highest clock frequency (high nibble)
					0xF_0 |
					// Lowest divide ratio (low nibble)
					0x0_0
				),
			]
		)?;
		send!(
			self,
			[
				// Set Second pre-charge Period
				0xB6, 0x04,
			]
		)?;
		send!(
			self,
			[
				// Set gamma LUT
				0xB8
			]
		)?;
		self.spi.transact_wo(&gamma_cmd[..])?;
		send!(
			self,
			[
				// Set pre-charge voltage level...
				0xBC, // ... 0.5*Vcc
				0x10,
			]
		)?;
		send!(
			self,
			[
				// Pre-charge voltage capacitor Selection
				0xBD, 0x01,
			]
		)?;
		send!(
			self,
			[
				// Set COM deselect voltage level
				0xBE, // ... 0.82*Vcc
				0x07,
			]
		)?;
		Ok(())
	}

	pub fn clear(&mut self) -> Result<(), SPI::Error> {
		cmd!(self);
		reset_cursor!(self)?;
		data!(self);
		let buf = [0u8; (256 / 2) * 64];
		self.spi.transact_wo(&buf[..])
	}

	pub fn on(&mut self) -> Result<(), SPI::Error> {
		cmd!(self);
		send!(
			self,
			[
				// Display ON
				0xAF
			]
		)
	}

	pub fn off(&mut self) -> Result<(), SPI::Error> {
		cmd!(self);
		send!(
			self,
			[
				// Display OFF (Sleep Mode)
				0xAE
			]
		)
	}

	pub fn paint(&mut self) -> Result<(), SPI::Error> {
		let buf = self.framebuf.data();
		cmd!(self);
		reset_cursor!(self)?;
		data!(self);
		self.spi.transact_wo(buf)
	}
}

impl<SPI: super::TransactWrite, DC: super::Pin> OriginDimensions for SSD1362<SPI, DC> {
	fn size(&self) -> Size {
		(256, 64).into()
	}
}

impl<SPI: super::TransactWrite, DC: super::Pin> DrawTarget for SSD1362<SPI, DC> {
	type Color = <FrameBuf as DrawTarget>::Color;
	type Error = <FrameBuf as DrawTarget>::Error;

	fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
	where
		I: IntoIterator<Item = Pixel<Self::Color>>,
	{
		FrameBuf::draw_iter::<I>(&mut self.framebuf, pixels)
	}

	fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
	where
		I: IntoIterator<Item = Self::Color>,
	{
		FrameBuf::fill_contiguous::<I>(&mut self.framebuf, area, colors)
	}
	fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
		FrameBuf::fill_solid(&mut self.framebuf, area, color)
	}
	fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
		FrameBuf::clear(&mut self.framebuf, color)
	}
}

impl<SPI: super::TransactWrite, DC: super::Pin> crate::chip::OledPeripheral for SSD1362<SPI, DC> {
	fn set_power_state(&mut self, state: crate::chip::OledPowerState) {
		// TODO implement the more nuanced types (requires modifying the SSD1362 crate)
		if state.simplified() {
			self.on().unwrap();
		} else {
			self.off().unwrap();
		}
	}
}

impl<SPI: super::TransactWrite, DC: super::Pin> crate::chip::BufferedDrawTarget
	for SSD1362<SPI, DC>
{
	type Error = SPI::Error;

	fn present(&mut self) -> Result<(), Self::Error> {
		self.paint()
	}
}
