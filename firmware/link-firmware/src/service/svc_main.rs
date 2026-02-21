use embassy_time::Timer;

pub struct Config {
	pub initialized: bool,
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, config: Config) -> ! {
	if !config.initialized {
		// Run the init service.
		bus.svc_init.send(super::svc_init::Cmd::Initialize).await;
		defmt::debug!("started the initialization service; halting main service");
		loop {
			Timer::after_secs(3600).await;
		}
	}

	bus.dev_blinken_light
		.send(super::dev_blinken_light::Cmd::Off)
		.await;

	defmt::error!("TODO: Main service");
	loop {
		Timer::after_secs(3600).await;
	}
}
