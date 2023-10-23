//! A [`Monitor`](crate::uc::Monitor) implementation for board configurations
//! with three indicator RGB LED situated to the right of a 256x64 OLED screen.
//! Assumes a Gray4 screen.

use crate::{
	chip::{BufferedDrawTarget, OledPeripheral, OledPowerState},
	font::{face, Font},
	uc::{
		helper::{
			oro_logo::OroLogo,
			three_indicators::{Color, IndicatorLights},
		},
		LogFrame, LogSeverity, Monitor, Scene,
	},
};
use embedded_graphics::{
	draw_target::DrawTarget, pixelcolor::Gray4, primitives::Rectangle, Drawable,
};
use heapless::{Deque, String};
use perlin2d::PerlinNoise2D;

const WHITE: Gray4 = Gray4::new(15);
const LIGHT_GRAY: Gray4 = Gray4::new(10);
const DARK_GRAY: Gray4 = Gray4::new(5);
const BLACK: Gray4 = Gray4::new(0);

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
	test_renderer: TestRenderer,
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
			test_renderer: Default::default(),
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
		if self.current_scene == Some(scene) {
			return;
		}

		match self.current_scene {
			None => {}
			Some(Scene::OroLogo) => self.logo_renderer.blur(),
			Some(Scene::Log) => self.log_renderer.blur(),
			Some(Scene::Test) => self.test_renderer.blur(),
		}

		self.current_scene = Some(scene);

		match scene {
			Scene::OroLogo => self
				.logo_renderer
				.focus(&mut self.indicators, &mut self.target),
			Scene::Log => self.log_renderer.focus(&mut self.indicators),
			Scene::Test => self.test_renderer.focus(&mut self.indicators),
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
				Scene::Log => self.log_renderer.tick(millis, &mut self.target),
				Scene::Test => {
					self.test_renderer
						.tick(millis, &mut self.target, &mut self.indicators)
				}
			}
		}
	}

	fn start_test_run(
		&mut self,
		total: usize,
		author: String<256>,
		title: String<256>,
		ref_id: String<256>,
	) {
		self.test_renderer
			.start_test_run(total, author, title, ref_id);
	}

	fn start_test(&mut self, name: String<256>) {
		self.test_renderer.start_test(name);
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

	fn focus<I: IndicatorLights, D: OledTarget>(&mut self, lights: &mut I, target: &mut D) {
		lights.enable();
		target.clear(BLACK).ok();
	}

	fn blur(&mut self) {}
}

#[derive(Default)]
struct LogRenderer {
	dirty: bool,
	entries: Deque<LogFrame, 4>,
}

impl LogRenderer {
	fn tick<D: OledTarget>(&mut self, _millis: u64, target: &mut D) {
		if !self.dirty {
			return;
		}

		self.dirty = false;

		target.clear(BLACK).ok();

		for (i, entry) in self.entries.iter().enumerate() {
			match entry.severity {
				LogSeverity::Info => face::TermNormal::draw_chars(
					entry.message.chars(),
					target,
					0,
					i as i32 * 16,
					DARK_GRAY,
					BLACK,
				),
				LogSeverity::Warn => face::TermBold::draw_chars(
					entry.message.chars(),
					target,
					0,
					i as i32 * 16,
					LIGHT_GRAY,
					BLACK,
				),
				LogSeverity::Error => face::TermBold::draw_chars(
					entry.message.chars(),
					target,
					0,
					i as i32 * 16,
					WHITE,
					BLACK,
				),
			};
		}

		target.present().unwrap();
	}

	fn push_log(&mut self, frame: LogFrame) {
		if self.entries.len() == 4 {
			self.entries.pop_front();
		}

		self.entries.push_back(frame).ok();
		self.dirty = true;
	}

	fn focus<I: IndicatorLights>(&mut self, lights: &mut I) {
		lights.disable();
		self.dirty = true;
	}

	fn blur(&mut self) {}
}

#[derive(Debug, Default)]
struct TestRenderer {
	total: usize,
	count: usize,
	author: String<256>,
	title: String<256>,
	ref_id: String<256>,
	current_test: String<256>,
	dirty: bool,
}

impl TestRenderer {
	fn tick<D: OledTarget, I: IndicatorLights>(
		&mut self,
		_millis: u64,
		target: &mut D,
		_lights: &mut I,
	) {
		if !self.dirty {
			return;
		}

		self.dirty = false;

		target.clear(BLACK).ok();

		face::TermBold::draw_chars(self.author.chars(), target, 0, 0, WHITE, BLACK);
		face::TermNormal::draw_chars(self.title.chars(), target, 0, 16, WHITE, BLACK);
		face::TermNormal::draw_chars(self.ref_id.chars(), target, 0, 32, LIGHT_GRAY, BLACK);
		face::TermNormal::draw_chars(self.current_test.chars(), target, 0, 48, DARK_GRAY, BLACK);

		let pct = ((self.count as i32 - 1).max(0) * 100) as usize / self.total;
		let pct_chars = [
			(b'0' + ((pct / 10) % 10) as u8) as char,
			(b'0' + (pct % 10) as u8) as char,
			'%',
		];
		let total_width: i32 = pct_chars
			.iter()
			.cloned()
			.map(face::Progress::char_width)
			.sum();
		let padded_width = total_width + 5;

		target
			.fill_solid(
				&Rectangle::new(
					(256i32 - padded_width, 0i32).into(),
					(padded_width as u32, 64u32).into(),
				),
				WHITE,
			)
			.ok();

		face::Progress::draw_chars(pct_chars, target, 256 - total_width, 0, WHITE, BLACK);

		target.present().ok();
	}

	fn focus<I: IndicatorLights>(&mut self, lights: &mut I) {
		lights.enable();
		self.dirty = true;
	}

	fn blur(&mut self) {}

	fn start_test_run(
		&mut self,
		total: usize,
		author: String<256>,
		title: String<256>,
		ref_id: String<256>,
	) {
		self.total = total;
		self.count = 0;
		self.author = author;
		self.title = title;
		self.ref_id = ref_id;
		self.dirty = true;
	}

	fn start_test(&mut self, name: String<256>) {
		self.current_test = name;
		self.dirty = true;
		self.count += 1;
	}
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
	#[inline(always)] // showed better results in Godbolt
	fn floor(self) -> f64 {
		let r = self % 1.0;
		if self < 0.0 {
			if r < 0.0 { self - (1.0 + r) } else { self }
		} else {
			self - r
		}
	}
}
