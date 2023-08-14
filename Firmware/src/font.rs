use embedded_graphics::{draw_target::DrawTarget, pixelcolor::PixelColor, Pixel};

pub mod face {
	include!(concat!(env!("OUT_DIR"), "/oro-link-fontdata.rs"));
}

pub trait FontData {
	fn charmap() -> &'static [char];
	fn idxmap() -> &'static [u16];
	fn data() -> &'static [u8];
}

pub trait Font {
	fn draw_char<PIX: PixelColor, TAR: DrawTarget<Color = PIX>>(
		chr: char,
		target: &mut TAR,
		x: i32,
		y: i32,
		white: PIX,
		black: PIX,
	) -> i32;

	fn char_width(chr: char) -> i32;

	fn draw_chars<PIX: PixelColor, TAR: DrawTarget<Color = PIX>, I: IntoIterator<Item = char>>(
		chrs: I,
		target: &mut TAR,
		mut x: i32,
		y: i32,
		white: PIX,
		black: PIX,
	) -> i32 {
		for c in chrs {
			match c {
				' ' => x += 2,
				'\t' => x = (x + 7) / 8 * 8,
				c => x += Self::draw_char(c, target, x, y, white, black),
			}
		}

		x
	}
}

impl<T: FontData> Font for T {
	fn char_width(chr: char) -> i32 {
		let idx = Self::charmap().binary_search(&chr).unwrap_or(0);
		let offset = Self::idxmap()[idx] as usize;
		Self::data()[offset + 1] as i32
	}

	fn draw_char<PIX: PixelColor, TAR: DrawTarget<Color = PIX>>(
		chr: char,
		target: &mut TAR,
		x: i32,
		y: i32,
		white: PIX,
		black: PIX,
	) -> i32 {
		let idx = Self::charmap().binary_search(&chr).unwrap_or(0);
		let offset = Self::idxmap()[idx] as usize;
		let top = Self::data()[offset] as usize;
		let width = Self::data()[offset + 1] as usize;
		let height = Self::data()[offset + 2] as usize;
		let total_pixels = width * height;
		let total_bytes = (total_pixels + 7) / 8;
		let byte_base = offset + 3;
		let bytes = &Self::data()[byte_base..byte_base + total_bytes];

		target
			.draw_iter((0..total_pixels).map(|i| {
				let x = (i % width) as i32 + x;
				let y = (i / width + top) as i32 + y;
				let bit = bytes[i / 8] >> (i % 8) & 1;
				Pixel((x, y).into(), if bit == 1 { white } else { black })
			}))
			.ok();

		width as i32
	}
}
