use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};

use super::dev_oled::FrameBuf;

mod logo;

pub type Channel = crate::channel::Channel<Cmd, 4>;

#[derive(defmt::Format, Clone, Copy, PartialEq, Eq, Default)]
#[allow(unused)]
pub enum Scene {
	#[default]
	Logo,
}

enum SceneState {
	Logo(logo::Scene),
}

impl SceneState {
	fn from_tag(scene: Scene) -> Self {
		match scene {
			Scene::Logo => Self::Logo(logo::Scene::default()),
		}
	}
}

pub enum Cmd {
	SetScene { scene: Scene },
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, rx: &'static Channel) -> ! {
	let mut current_state = SceneState::from_tag(Scene::default());

	loop {
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

			if let Either::First(cmd) = select(rx.receive(), Timer::after(frame_delay)).await {
				break cmd;
			}
		};

		match cmd {
			Cmd::SetScene { scene } => {
				current_state = SceneState::from_tag(scene);
			}
		}
	}
}

pub trait RenderScene {
	fn render(&mut self, fb: &mut FrameBuf) -> Duration;
}

impl RenderScene for SceneState {
	fn render(&mut self, fb: &mut FrameBuf) -> Duration {
		match self {
			SceneState::Logo(s) => s.render(fb),
		}
	}
}
