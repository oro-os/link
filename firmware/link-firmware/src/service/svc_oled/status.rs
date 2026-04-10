use embassy_time::Duration;
use embedded_graphics::{pixelcolor::Gray4, prelude::*};

use crate::{
	font::{Font, face},
	service::dev_oled::FrameBuf,
};

pub struct Scene(pub super::Status);

#[macro_export]
macro_rules! oled_status {
	(
		$bus:expr,
		$style1:ident($line1:expr),
		$style2:ident($line2:expr),
		$style3:ident($line3:expr),
		$style4:ident($line4:expr) $(,)?
	) => {
		$bus.svc_oled
			.send($crate::service::svc_oled::Cmd::SetScene {
				scene: $crate::service::svc_oled::Scene::Status(
					$crate::service::svc_oled::Status {
						line1: Some($crate::service::svc_oled::Line::$style1($line1)),
						line2: Some($crate::service::svc_oled::Line::$style2($line2)),
						line3: Some($crate::service::svc_oled::Line::$style3($line3)),
						line4: Some($crate::service::svc_oled::Line::$style4($line4)),
					},
				),
			})
			.await
	};

	(
		$bus:expr,
		$style1:ident($line1:expr),
		$style2:ident($line2:expr),
		$style3:ident($line3:expr) $(,)?
	) => {
		$bus.svc_oled
			.send($crate::service::svc_oled::Cmd::SetScene {
				scene: $crate::service::svc_oled::Scene::Status(
					$crate::service::svc_oled::Status {
						line1: Some($crate::service::svc_oled::Line::$style1($line1)),
						line2: Some($crate::service::svc_oled::Line::$style2($line2)),
						line3: Some($crate::service::svc_oled::Line::$style3($line3)),
						..Default::default()
					},
				),
			})
			.await
	};

	($bus:expr, $style1:ident($line1:expr), $style2:ident($line2:expr) $(,)?) => {
		$bus.svc_oled
			.send($crate::service::svc_oled::Cmd::SetScene {
				scene: $crate::service::svc_oled::Scene::Status(
					$crate::service::svc_oled::Status {
						line2: Some($crate::service::svc_oled::Line::$style1($line1)),
						line3: Some($crate::service::svc_oled::Line::$style2($line2)),
						..Default::default()
					},
				),
			})
			.await
	};

	($bus:expr, $style:ident($line:expr) $(,)?) => {
		$bus.svc_oled
			.send($crate::service::svc_oled::Cmd::SetScene {
				scene: $crate::service::svc_oled::Scene::Status(
					$crate::service::svc_oled::Status {
						line2: Some($crate::service::svc_oled::Line::$style($line)),
						..Default::default()
					},
				),
			})
			.await
	};
}

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
