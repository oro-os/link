use embassy_time::Timer;

pub struct Config {}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, _config: Config) -> ! {
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

	defmt::error!("TODO: Main service");
	loop {
		Timer::after_secs(3600).await;
	}
}
