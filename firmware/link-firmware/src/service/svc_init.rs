use embassy_time::Timer;
use link_protocol::{Request, Response};

use crate::flash::PflashV0;

pub type Channel = crate::channel::Channel<Cmd, 2>;
pub enum Cmd {
	Initialize,
}

pub struct Config {
	pub pflash: PflashV0,
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, rx: &'static Channel, config: Config) -> ! {
	let Config { mut pflash } = config;

	let Cmd::Initialize = rx.receive().await;
	defmt::info!("running initialization service");

	bus.dev_blinken_light
		.send(super::dev_blinken_light::Cmd::Config)
		.await;
	bus.svc_oled_pwr
		.send(super::svc_oled_pwr::Cmd::SetState {
			state: super::svc_oled_pwr::State::On,
		})
		.await;
	bus.svc_oled
		.send(super::svc_oled::Cmd::SetScene {
			scene: super::svc_oled::Scene::Status(super::svc_oled::Status {
				line2: Some(super::svc_oled::Line::Bold("Oro Link Setup")),
				line3: Some(super::svc_oled::Line::Normal(
					"Visit oro.sh/link to set up this Link.",
				)),
				..Default::default()
			}),
		})
		.await;

	let mut repl_active = true;
	while repl_active {
		defmt::debug!("waiting for uart request...");
		let res = match super::dev_uart::PACKET.wait().await {
			Request::GetVersionMajor => Response::Uint(crate::version::VERSION_MAJOR),
			Request::GetVersionMinor => Response::Uint(crate::version::VERSION_MINOR),
			Request::GetVersionPatch => Response::Uint(crate::version::VERSION_PATCH),
			Request::IsInInitMode => Response::Uint(1),
			Request::FinishInitMode => {
				repl_active = false;
				Response::Ok
			}
			Request::FactoryReset => {
				defmt::warn!("oro link is factory resetting");
				pflash = PflashV0::default();
				Response::Ok
			}
		};

		bus.dev_uart.send(super::dev_uart::Cmd::Send(res)).await;
	}

	defmt::warn!("init service is finished; writing the pflash and resetting");
	set_initialized_and_reset(pflash).await;
}

async fn set_initialized_and_reset(mut pflash: PflashV0) -> ! {
	pflash.initialized = true;
	// SAFETY: We're initializing it.
	if let Err(err) = unsafe { crate::flash::write_pflash(pflash) } {
		defmt::error!("failed to write pflash during first-time setup: {:?}", err);
		defmt::error!("halting system");
		panic!("failed to initialize system");
	}
	defmt::info!("first-time setup complete");

	Timer::after_millis(100).await;

	// SAFETY: We're resetting the system.
	unsafe { crate::reset() }
}
