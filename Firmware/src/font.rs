use embedded_graphics::{draw_target::DrawTarget, pixelcolor::PixelColor, Pixel};

pub mod face {
	include!(concat!(env!("OUT_DIR"), "/oro-link-fontdata.rs"));
}

pub trait Font<const CHARS: usize, const BYTES: usize> {
	const CHARMAP: [char; CHARS];
	const IDXMAP: [u16; CHARS];
	const DATA: [u8; BYTES];

	fn draw_char<PIX: PixelColor, TAR: DrawTarget<Color = PIX>>(
		chr: char,
		target: &mut TAR,
		x: i32,
		y: i32,
		white: PIX,
		black: PIX,
	) -> i32 {
		let idx = Self::CHARMAP.binary_search(&chr).unwrap_or(0);
		let offset = Self::IDXMAP[idx] as usize;
		let top = Self::DATA[offset] as usize;
		let width = Self::DATA[offset + 1] as usize;
		let height = Self::DATA[offset + 2] as usize;
		let total_pixels = width * height;
		let total_bytes = (total_pixels + 7) / 8;
		let byte_base = offset + 3;
		let bytes = &Self::DATA[byte_base..byte_base + total_bytes];

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
