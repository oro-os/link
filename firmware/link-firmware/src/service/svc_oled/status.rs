use embassy_time::Duration;
use embedded_graphics::{pixelcolor::Gray4, prelude::*};

use crate::{
	font::{Font, face},
	service::dev_oled::FrameBuf,
};

pub struct Scene(pub super::Status);

impl super::RenderScene for Scene {
	fn render(&mut self, fb: &mut FrameBuf) -> Option<Duration> {
		let this = &self.0;

		this.line1.as_ref().inspect(|l| fb.draw_line(l, 0));
		this.line2.as_ref().inspect(|l| fb.draw_line(l, 16));
		this.line3.as_ref().inspect(|l| fb.draw_line(l, 32));
		this.line4.as_ref().inspect(|l| fb.draw_line(l, 48));

		None
	}
}

trait DrawLine: DrawTarget<Color = Gray4> + Sized {
	fn draw_line(&mut self, line: &super::Line, y: i32) {
		match line {
			super::Line::Normal(s) => {
				let width = face::TermNormal::str_width(s.chars());
				face::TermNormal::draw_chars(
					s.chars(),
					self,
					128 - (width >> 1),
					y,
					Gray4::WHITE,
					Gray4::BLACK,
				);
			}
			super::Line::Bold(s) => {
				let width = face::TermBold::str_width(s.chars());
				face::TermBold::draw_chars(
					s.chars(),
					self,
					128 - (width >> 1),
					y,
					Gray4::WHITE,
					Gray4::BLACK,
				);
			}
		}
	}
}

impl<T: DrawTarget<Color = Gray4> + Sized> DrawLine for T {}
