use embassy_time::Timer;

pub struct Config {}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, _config: Config) -> ! {
	bus.dev_blinken_light
		.send(super::dev_blinken_light::Cmd::Off)
		.await;

	defmt::error!("TODO: Main service");
	loop {
		Timer::after_secs(3600).await;
	}
}
