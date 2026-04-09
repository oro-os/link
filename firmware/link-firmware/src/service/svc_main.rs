use embassy_sync::once_lock::OnceLock;
use embassy_time::Timer;

use crate::service::svc_mqtt::Mqtt;

pub struct Config {
	pub mqtt: &'static OnceLock<Mqtt>,
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, config: Config) -> ! {
	let Config { mqtt } = config;

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

	// Let the logo show up :D
	Timer::after_secs(2).await;

	// Wait for MQTT to connect
	bus.svc_oled
		.send(super::svc_oled::Cmd::SetScene {
			scene: super::svc_oled::Scene::Status(super::svc_oled::Status {
				line2: Some(super::svc_oled::Line::Bold("Waiting for MQTT...")),
				line3: Some(super::svc_oled::Line::Normal(crate::unique_id())),
				..Default::default()
			}),
		})
		.await;

	mqtt.get().await;

	bus.dev_blinken_light
		.send(super::dev_blinken_light::Cmd::Idle)
		.await;

	// SAFETY: keep the number of seconds in the single digits.
	#[expect(static_mut_refs)]
	for s in (1..=5).rev() {
		static mut TIMEOUT_MSG: heapless::String<9> = heapless::String::new();
		// SAFETY: this is the only place it's used and it's used in lockstep;
		// SAFETY: technically UB but not a problem in this very specific case.
		// SAFETY: if the seconds is ever increased to double digits, a race
		// SAFETY: condition could occur, so don't do that.
		unsafe {
			TIMEOUT_MSG = heapless::format!("in {s}s...").unwrap();
		}
		bus.svc_oled
			.send(super::svc_oled::Cmd::SetScene {
				scene: super::svc_oled::Scene::Status(super::svc_oled::Status {
					line2: Some(super::svc_oled::Line::Bold("MQTT connected; standing by")),
					line3: Some(super::svc_oled::Line::Normal(unsafe {
						TIMEOUT_MSG.as_str()
					})),
					..Default::default()
				}),
			})
			.await;

		Timer::after_secs(1).await;
	}

	loop {
		bus.svc_oled
			.send(super::svc_oled::Cmd::SetScene {
				scene: super::svc_oled::Scene::Logo,
			})
			.await;

		bus.svc_oled_pwr
			.send(super::svc_oled_pwr::Cmd::SetState {
				state: super::svc_oled_pwr::State::Idle,
			})
			.await;

		defmt::info!("waiting for PR...");
		if !super::svc_mqtt_config::CFG_PR_RUN.next().await {
			defmt::debug!("pr sent false; ignoring");
			continue;
		}

		bus.svc_oled_pwr
			.send(super::svc_oled_pwr::Cmd::SetState {
				state: super::svc_oled_pwr::State::On,
			})
			.await;

		// Wait for OLED to turn on
		Timer::after_millis(250).await;

		defmt::info!("PR run started");
		bus.svc_oled
			.send(super::svc_oled::Cmd::SetScene {
				scene: super::svc_oled::Scene::Status(super::svc_oled::Status {
					line2: Some(super::svc_oled::Line::Bold(
						"PR started; fetching configuration",
					)),
					line3: Some(super::svc_oled::Line::Normal("pr/title")),
					..Default::default()
				}),
			})
			.await;

		// XXX
		Timer::after_secs(10).await;
	}
}
