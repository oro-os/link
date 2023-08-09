//! A [`Monitor`](crate::uc::Monitor) implementation for board configurations
//! with three indicator RGB LED situated to the right of a 256x64 OLED screen.
//! Assumes a Gray4 screen.

use crate::{
	chip::{BufferedDrawTarget, OledPeripheral, OledPowerState},
	uc::{
		helper::{
			oro_logo::OroLogo,
			three_indicators::{Color, IndicatorLights},
		},
		LogFrame, Monitor, Scene,
	},
};
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Gray4, Drawable};
use perlin2d::PerlinNoise2D;

pub trait OledTarget: DrawTarget<Color = Gray4> + BufferedDrawTarget + OledPeripheral {}

impl<T> OledTarget for T where T: DrawTarget<Color = Gray4> + BufferedDrawTarget + OledPeripheral {}

pub struct ThreeIndicatorsOled256x64<D, I>
where
	D: OledTarget,
	I: IndicatorLights,
{
	indicators: I,
	target: D,
	current_scene: Option<Scene>,
	logo_renderer: OroLogoRenderer,
	log_renderer: LogRenderer,
}

impl<D, I> ThreeIndicatorsOled256x64<D, I>
where
	D: OledTarget,
	I: IndicatorLights,
{
	pub fn new(target: D, indicators: I) -> Self {
		let mut s = Self {
			target,
			indicators,
			current_scene: None,
			logo_renderer: Default::default(),
			log_renderer: Default::default(),
		};

		// Focus the first scene
		s.set_scene(Default::default());

		s
	}
}

impl<D, I> Monitor for ThreeIndicatorsOled256x64<D, I>
where
	D: OledTarget,
	I: IndicatorLights,
{
	fn standby_mode(&mut self, enable: bool) {
		if enable {
			self.target.set_power_state(OledPowerState::On);
		} else {
			self.target.set_power_state(OledPowerState::FadeOut);
		}
	}

	fn set_scene(&mut self, scene: Scene) {
		match self.current_scene {
			None => {}
			Some(Scene::OroLogo) => self.logo_renderer.blur(),
			Some(Scene::Log) => self.log_renderer.blur(),
		}

		self.current_scene = Some(scene);

		match scene {
			Scene::OroLogo => self.logo_renderer.focus(&mut self.indicators),
			Scene::Log => self.log_renderer.focus(),
		}
	}

	fn push_log(&mut self, frame: LogFrame) {
		self.log_renderer.push_log(frame);
	}

	fn tick(&mut self, millis: u64) {
		if let Some(current_scene) = self.current_scene.as_mut() {
			match current_scene {
				Scene::OroLogo => {
					self.logo_renderer
						.tick(millis, &mut self.target, &mut self.indicators)
				}
				Scene::Log => self.log_renderer.tick(millis),
			}
		}
	}
}

struct OroLogoRenderer {
	logo: OroLogo,
	next_frame_at: u64,
	noisegen: PerlinNoise2D,
}

impl Default for OroLogoRenderer {
	fn default() -> Self {
		Self {
			noisegen: PerlinNoise2D::new(10, 1.0, 0.5, 1.0, 1.0, (100.0, 100.0), 0.0, 101),
			next_frame_at: 0u64,
			logo: OroLogo::new(((256 - 64) / 2, 0).into()),
		}
	}
}

impl OroLogoRenderer {
	fn tick<D: OledTarget, I: IndicatorLights>(
		&mut self,
		millis: u64,
		target: &mut D,
		lights: &mut I,
	) {
		// Calculate indicator light values.
		const FLICKER_AMOUNT: f64 = 0.6;
		const TIME_SCALE: f64 = 1.1;
		const LIGHT_OFFSET: f64 = 100.0; // ms, scaled inversely to TIME_SCALE
		let millisf = millis as f64;
		let light_offset = self.noisegen.get_noise(millisf * 1.5, 0.0);
		let light_offset = (light_offset * 3.0 * LIGHT_OFFSET)
			.min(LIGHT_OFFSET)
			.max(-LIGHT_OFFSET);

		for i in 0..3 {
			let fi = i as f64;
			// NOTE: n is not guaranteed to be -1..1.
			let n = self
				.noisegen
				.get_noise(0.0, millisf + (fi * light_offset) * TIME_SCALE);
			let on = n * 0.8;
			let n = (n * 0.5).min(1.0).max(-1.0);
			let n = n * 0.5 + 0.5;
			let v = n * FLICKER_AMOUNT + (1.0 - FLICKER_AMOUNT);
			let v = if v < 0.3 {
				v * v * v * v * v * v * v * v * v * v * v * v * v * v * v * v * v * v
			} else {
				(v * v * v * v * v * v * v * v * v) * 0.7 + 0.3
			};
			let v = v * v * v;
			let s = v * 0.85;
			let s = s * s;
			let h = (on - 1.6).min(1.0).max(0.0) * (0.805 - 0.402) + 0.402;
			let v = (v - (on * 0.04)).max(0.0).min(1.0);
			let mut color: Color = hsv_to_rgb(h, s, v).into();

			// Adjust GB to match R luminance
			color.g = (color.g as f64 * 0.5) as u8;
			color.b = (color.b as f64 * 0.5) as u8;

			match i {
				0 => lights.first(color),
				1 => lights.second(color),
				2 => lights.third(color),
				_ => unreachable!(),
			}
		}

		// Advance next frame if need be
		if self.next_frame_at <= millis {
			self.next_frame_at += 1000 / OroLogo::fps();
			self.logo.advance();
			self.logo.draw(target).ok();
			target.present().unwrap();
		}
	}

	fn invalidate<D: OledTarget>(&mut self, target: &mut D) {}

	fn focus<I: IndicatorLights>(&mut self, lights: &mut I) {
		lights.enable();
	}

	fn blur(&mut self) {}
}

#[derive(Default)]
struct LogRenderer {}

impl LogRenderer {
	fn tick(&mut self, millis: u64) {}

	fn invalidate(&mut self) {}

	fn push_log(&mut self, frame: LogFrame) {}

	fn focus(&mut self) {}

	fn blur(&mut self) {}
}

/// Ported from <https://github.com/Qix-/color-convert/blob/master/conversions.js>
///
/// All domains are `0..=1`.
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
	let h = (h * 360.0) / 60.0;
	let hf = h.floor();
	let hi = hf % 6.0;

	let f = h - hf;
	let p = 255.0 * v * (1.0 - s);
	let q = 255.0 * v * (1.0 - (s * f));
	let t = 255.0 * v * (1.0 - (s * (1.0 - f)));
	let v = v * 255.0;

	let hi = hi as u8;
	let v = v as u8;
	let t = t as u8;
	let p = p as u8;
	let q = q as u8;

	match hi {
		0 => (v, t, p),
		1 => (q, v, p),
		2 => (p, v, t),
		3 => (p, q, v),
		4 => (t, p, v),
		5 => (v, p, q),
		_ => unreachable!(),
	}
}

/// Re-implements floor for f64's in no_std environments
trait F64Floor {
	fn floor(self) -> f64;
}

impl F64Floor for f64 {
	fn floor(self) -> f64 {
		let r = self % 1.0;
		if self < 0.0 {
			if r < 0.0 { self - (1.0 + r) } else { self }
		} else {
			self - r
		}
	}
}
