//! SSD1362-based OLED screens
//! Requires SPI + D/C line ("4-wire SPI")

use display_interface::{DataFormat, DisplayError, WriteOnlyDataCommand};
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
use ssd1362::display::{Display, DisplayAddressing, DisplayRotation};

type FrameBuf = Framebuffer<
	Gray4,
	<Gray4 as PixelColor>::Raw,
	LittleEndian,
	256,
	64,
	{ buffer_size::<Gray4>(256, 64) },
>;

struct SpiDc<SPI: super::TransactWrite, DC: super::Pin>(SPI, DC);

/// Implements low-level communication for SSD1362-based
/// OLED/LCD/LED screens.
pub struct SSD1362<SPI: super::TransactWrite, DC: super::Pin>(Display<SpiDc<SPI, DC>>, FrameBuf);

impl<SPI: super::TransactWrite, DC: super::Pin> SSD1362<SPI, DC> {
	pub fn new(spi: SPI, dc: DC, flip: bool) -> Result<Self, DisplayError> {
		let mut display = Display::new(
			SpiDc(spi, dc),
			if flip {
				DisplayRotation::Rotate180
			} else {
				DisplayRotation::Rotate0
			},
			DisplayAddressing::Horizontal,
		);

		display.init()?;

		// We only support either 256x64 or 64x256
		let (width, height) = display.dimensions();
		assert_eq!(width, 256);
		assert_eq!(height, 64);

		Ok(Self(display, FrameBuf::new()))
	}

	pub fn clear(&mut self) -> Result<(), DisplayError> {
		self.0.blank()
	}

	pub fn on(&mut self) -> Result<(), DisplayError> {
		self.0.on()
	}

	pub fn off(&mut self) -> Result<(), DisplayError> {
		self.0.off()
	}
}

impl<SPI: super::TransactWrite, DC: super::Pin> SpiDc<SPI, DC> {
	fn send(&mut self, cmd: DataFormat) -> Result<(), DisplayError> {
		// SSD1362 crate only uses U8; this method will have to be updated if it uses anything else.
		if let DataFormat::U8(buf) = cmd {
			self.0
				.transact_wo(buf)
				.map_err(|_| DisplayError::BusWriteError)
		} else {
			Err(DisplayError::InvalidFormatError)
		}
	}
}

impl<SPI: super::TransactWrite, DC: super::Pin> WriteOnlyDataCommand for SpiDc<SPI, DC> {
	fn send_commands(&mut self, cmd: DataFormat) -> Result<(), DisplayError> {
		self.1.low();
		self.send(cmd)
	}

	fn send_data(&mut self, buf: DataFormat) -> Result<(), DisplayError> {
		self.1.high();
		self.send(buf)
	}
}

impl<SPI: super::TransactWrite, DC: super::Pin> OriginDimensions for SSD1362<SPI, DC> {
	fn size(&self) -> Size {
		OriginDimensions::size(&self.1)
	}
}

impl<SPI: super::TransactWrite, DC: super::Pin> DrawTarget for SSD1362<SPI, DC> {
	type Color = <FrameBuf as DrawTarget>::Color;
	type Error = <FrameBuf as DrawTarget>::Error;

	fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
	where
		I: IntoIterator<Item = Pixel<Self::Color>>,
	{
		FrameBuf::draw_iter::<I>(&mut self.1, pixels)
	}

	fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
	where
		I: IntoIterator<Item = Self::Color>,
	{
		FrameBuf::fill_contiguous::<I>(&mut self.1, area, colors)
	}
	fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
		FrameBuf::fill_solid(&mut self.1, area, color)
	}
	fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
		FrameBuf::clear(&mut self.1, color)
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
	type Error = DisplayError;

	fn present(&mut self) -> Result<(), Self::Error> {
		self.0.draw(self.1.data())
	}
}
