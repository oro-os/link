use embassy_time::Timer;

use crate::flash::PflashV0;

pub type Channel = crate::channel::Channel<Cmd, 2>;
pub enum Cmd {
	Start,
	Finish,
	FactoryReset,
}

pub struct Config {
	pub pflash: PflashV0,
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, rx: &'static Channel, config: Config) -> ! {
	let Config { mut pflash } = config;

	loop {
		let Cmd::Start = rx.receive().await else {
			defmt::warn!("init service received stray event but is not started; expected Start");
			continue;
		};
		break;
	}
	defmt::info!("running initialization service");

	bus.svc_uart
		.send(super::svc_uart::Cmd::SetInitMode { in_init_mode: true })
		.await;

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
		match rx.receive().await {
			Cmd::Start => {
				defmt::warn!("received Start but Init is already started");
			}
			Cmd::Finish => {
				repl_active = false;
			}
			Cmd::FactoryReset => {
				defmt::warn!("oro link is factory resetting");
				pflash = PflashV0::default();
			}
		}
	}

	defmt::warn!("init service is finished; writing the pflash and resetting");
	bus.svc_uart
		.send(super::svc_uart::Cmd::SetInitMode {
			in_init_mode: false,
		})
		.await;
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
