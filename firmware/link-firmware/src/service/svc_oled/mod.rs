use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use embedded_graphics::{
	pixelcolor::Gray4,
	prelude::{DrawTarget, GrayColor},
};

use super::dev_oled::FrameBuf;

mod logo;
mod status;

pub type Channel = crate::channel::Channel<Cmd, 4>;

pub enum Line {
	Normal(&'static str),
	Bold(&'static str),
}

#[derive(Default)]
pub struct Status {
	pub line1: Option<Line>,
	pub line2: Option<Line>,
	pub line3: Option<Line>,
	pub line4: Option<Line>,
}

#[derive(Default)]
#[allow(unused)]
pub enum Scene {
	#[default]
	Logo,
	Status(Status),
}

enum SceneState {
	Logo(logo::Scene),
	Status(status::Scene),
}

pub enum Cmd {
	SetScene { scene: Scene },
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, rx: &'static Channel) -> ! {
	let mut current_state = SceneState::Logo(logo::Scene::new());

	loop {
		{
			super::dev_oled::FRAME_BUFFER
				.lock()
				.await
				.clear(Gray4::BLACK);
		}

		let cmd = loop {
			// Wait for a handle to the frame renderer
			let mut frame_buffer =
				match select(rx.receive(), super::dev_oled::FRAME_BUFFER.lock()).await {
					Either::First(cmd) => break cmd,
					Either::Second(fb) => fb,
				};

			let frame_delay = current_state.render(&mut frame_buffer);
			drop(frame_buffer);

			bus.dev_oled.send(super::dev_oled::Cmd::Render).await;

			if let Some(frame_delay) = frame_delay {
				if let Either::First(cmd) = select(rx.receive(), Timer::after(frame_delay)).await {
					break cmd;
				}
			} else {
				break rx.receive().await;
			}
		};

		match cmd {
			Cmd::SetScene { scene: Scene::Logo } => {
				current_state = SceneState::Logo(logo::Scene::new());
			}
			Cmd::SetScene {
				scene: Scene::Status(status),
			} => {
				current_state = SceneState::Status(status::Scene(status));
			}
		}
	}
}

pub trait RenderScene {
	fn render(&mut self, fb: &mut FrameBuf) -> Option<Duration>;
}

impl RenderScene for SceneState {
	fn render(&mut self, fb: &mut FrameBuf) -> Option<Duration> {
		match self {
			SceneState::Logo(s) => s.render(fb),
			SceneState::Status(s) => s.render(fb),
		}
	}
}
