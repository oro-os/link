use embassy_time::Timer;

use crate::flash::PflashV0;

pub type Channel = crate::channel::Channel<Cmd, 2>;
pub enum Cmd {
	Initialize,
}

pub struct Config {
	pub pflash: PflashV0,
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, rx: &'static Channel, #[allow(unused)] config: Config) -> ! {
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
			scene: super::svc_oled::Scene::Logo,
		})
		.await;

	loop {
		Timer::after_secs(1000).await;
	}

	#[expect(unreachable_code)]
	{
		defmt::warn!("init service is finished; writing the pflash and resetting");
		set_initialized_and_reset(config.pflash).await;
	}
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
