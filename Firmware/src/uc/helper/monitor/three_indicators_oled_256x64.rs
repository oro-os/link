use crate::{
	chip::{BufferedDrawTarget, OledPeripheral, OledPowerState},
	uc::{
		helper::three_indicators::{Color, IndicatorLights},
		LogFrame, Monitor, Scene,
	},
};
/// A [`Monitor`](crate::uc::Monitor) implementation for board configurations
/// with three indicator RGB LED situated to the right of a 256x64 OLED screen.
/// Assumes a Gray4 screen.
use embedded_graphics_core::{draw_target::DrawTarget, pixelcolor::Gray4};

pub trait OledTarget: DrawTarget<Color = Gray4> + BufferedDrawTarget + OledPeripheral {}

impl<T> OledTarget for T where T: DrawTarget<Color = Gray4> + BufferedDrawTarget + OledPeripheral {}

pub struct ThreeIndicatorsOled256x64<D, I>
where
	D: OledTarget,
	I: IndicatorLights,
{
	indicators: I,
	target: D,
	current_scene: Scene,
	logo_renderer: OroLogoRenderer,
	log_renderer: LogRenderer,
}

impl<D, I> ThreeIndicatorsOled256x64<D, I>
where
	D: OledTarget,
	I: IndicatorLights,
{
	pub fn new(target: D, indicators: I) -> Self {
		Self {
			target,
			indicators,
			current_scene: Default::default(),
			logo_renderer: Default::default(),
			log_renderer: Default::default(),
		}
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
		self.current_scene = scene;

		match self.current_scene {
			Scene::OroLogo => self.logo_renderer.invalidate(&mut self.target),
			Scene::Log => self.log_renderer.invalidate(),
		}
	}

	fn push_log(&mut self, frame: LogFrame) {
		self.log_renderer.push_log(frame);
	}

	fn tick(&mut self, millis: u64) {
		match self.current_scene {
			Scene::OroLogo => self.logo_renderer.tick(millis, &mut self.target),
			Scene::Log => self.log_renderer.tick(millis),
		}
	}
}

#[derive(Default)]
struct OroLogoRenderer {}

impl OroLogoRenderer {
	fn tick<D: OledTarget>(&mut self, millis: u64, target: &mut D) {
		// TODO XXX DEBUG
		use embedded_graphics::primitives::rectangle::Rectangle;
		target.clear(Gray4::new(0)).ok();
		target
			.fill_solid(
				&Rectangle::new((0, 0).into(), (21, 15).into()),
				Gray4::new(15),
			)
			.ok();
		target.present().unwrap();
	}

	fn invalidate<D: OledTarget>(&mut self, target: &mut D) {}
}

#[derive(Default)]
struct LogRenderer {}

impl LogRenderer {
	fn tick(&mut self, millis: u64) {}

	fn invalidate(&mut self) {}

	fn push_log(&mut self, frame: LogFrame) {}
}
