use embassy_time::Duration;
use embedded_graphics::{pixelcolor::Gray4, prelude::*};

use crate::service::dev_oled::FrameBuf;

type OroLogo = oro_logo_rle::OroLogo<oro_logo_rle::OroLogo64x64>;

const ORO_LOGO_COLORS: &[Gray4] = &[
	Gray4::new(0x0),
	Gray4::new(0x5),
	Gray4::new(0xA),
	Gray4::new(0xF),
];

pub struct Scene {
	logo_iter: OroLogo,
}

impl Default for Scene {
	fn default() -> Self {
		Self {
			logo_iter: OroLogo::new(),
		}
	}
}

impl super::RenderScene for Scene {
	fn render(&mut self, fb: &mut FrameBuf) -> Duration {
		use oro_logo_rle::{Command, OroLogoData};

		let mut off = 0;

		loop {
			match self.logo_iter.next() {
				None => panic!("Oro logo exhausted commands (shouldn't happen)"),

				Some(Command::End) => break,

				Some(Command::Draw(count, lightness)) => {
					let color = ORO_LOGO_COLORS[usize::from(lightness)];

					for i in 0..count {
						let x = ((off + usize::from(i)) % OroLogo::WIDTH)
							+ ((256 / 2) - (OroLogo::WIDTH / 2));
						let y = (off + usize::from(i)) / OroLogo::WIDTH;
						fb.set_pixel(Point::new(x as i32, y as i32), color);
					}

					off += usize::from(count);
				}

				Some(Command::Skip(count)) => {
					off += usize::from(count);
				}
			}
		}

		Duration::from_millis(1000 / OroLogo::FPS as u64)
	}
}
