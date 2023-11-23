//! [`embedded_graphics::Drawable`] implementation for the Oro Logo
//!
//! Must have **ONLY ONE** `oro-logo-*` feature enabled.

use embedded_graphics::{
	draw_target::DrawTarget, geometry::Point, pixelcolor::Gray4, Drawable, Pixel,
};
use oro_logo_rle::{Command, OroLogo as Logo, OroLogoData};

#[cfg(feature = "oro-logo-1024")]
type OroLogoSized = oro_logo_rle::OroLogo1024x1024;
#[cfg(feature = "oro-logo-512")]
type OroLogoSized = oro_logo_rle::OroLogo512x512;
#[cfg(feature = "oro-logo-256")]
type OroLogoSized = oro_logo_rle::OroLogo256x256;
#[cfg(feature = "oro-logo-64")]
type OroLogoSized = oro_logo_rle::OroLogo64x64;
#[cfg(feature = "oro-logo-32")]
type OroLogoSized = oro_logo_rle::OroLogo32x32;

const C0: Gray4 = Gray4::new(0);
const C1: Gray4 = Gray4::new(255 / 3);
const C2: Gray4 = Gray4::new(255 / 3 * 2);
const C3: Gray4 = Gray4::new(255);

pub struct OroLogo {
	logo: Logo<OroLogoSized>,
	buffer: [Gray4; OroLogoSized::HEIGHT * OroLogoSized::HEIGHT],
	pos: Point,
}

#[allow(unused)]
impl OroLogo {
	pub fn new(position: Point) -> Self {
		Self {
			logo: Logo::new(),
			buffer: [C0; OroLogoSized::HEIGHT * OroLogoSized::HEIGHT],
			pos: position,
		}
	}

	pub fn position(&self) -> Point {
		self.pos
	}

	pub fn set_position(&mut self, position: Point) {
		self.pos = position;
	}

	pub const fn fps() -> u64 {
		OroLogoSized::FPS as u64
	}

	pub fn advance(&mut self) {
		let mut off = 0usize;

		loop {
			match self.logo.next() {
				None => defmt::panic!("Oro logo exhausted commands (shouldn't happen)"),
				Some(Command::End) => break,
				Some(Command::Draw(count, lightness)) => {
					let color = match lightness {
						0 => C0,
						1 => C1,
						2 => C2,
						3 => C3,
						_ => unreachable!(),
					};
					for i in 0..count {
						self.buffer[off + (i as usize)] = color;
					}
					off += count as usize;
				}
				Some(Command::Skip(count)) => {
					off += count as usize;
				}
			}
		}
	}
}

impl Drawable for OroLogo {
	type Color = Gray4;
	type Output = ();

	fn draw<D>(&self, target: &mut D) -> Result<Self::Output, <D as DrawTarget>::Error>
	where
		D: DrawTarget<Color = Self::Color>,
	{
		let origin = self.pos;
		target.draw_iter(self.buffer.iter().enumerate().map(|(i, c)| {
			Pixel(
				origin
					+ Point {
						x: (i % OroLogoSized::WIDTH) as _,
						y: (i / OroLogoSized::WIDTH) as _,
					},
				*c,
			)
		}))
	}
}
