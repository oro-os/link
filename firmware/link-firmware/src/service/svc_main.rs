use embassy_sync::once_lock::OnceLock;
use embassy_time::Timer;

use crate::{color::Rgb, service::svc_mqtt::Mqtt};

pub struct Config {
	pub mqtt: &'static OnceLock<Mqtt>,
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, config: Config) -> ! {
	let Config { mqtt } = config;

	// Initial LED state
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

	// Perform LED self-test
	bus.dev_leds.send(super::dev_leds::Cmd::SelfTest).await;

	// Wait for self-test to complete (and let the logo show up :D)
	Timer::after_secs(5).await;

	// Wait for MQTT to connect
	bus.dev_leds.send(super::dev_leds::Cmd::AllOff).await;
	bus.dev_leds
		.send(super::dev_leds::Cmd::SetSystemIndicator(Rgb::new(2, 18, 2)))
		.await;
	bus.dev_leds
		.send(super::dev_leds::Cmd::SetRemoteIndicator(Rgb::new(
			245, 65, 5,
		)))
		.await;

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

	bus.dev_leds
		.send(super::dev_leds::Cmd::SetRemoteIndicator(Rgb::new(2, 18, 2)))
		.await;

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
		bus.dev_leds
			.send(super::dev_leds::Cmd::SetJobIndicator(Rgb::new(2, 1, 1)))
			.await;

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
		bus.svc_leds
			.send(super::svc_leds::Cmd::SetState {
				state: super::svc_leds::State::Off,
			})
			.await;
		bus.dev_blinken_light
			.send(super::dev_blinken_light::Cmd::Idle)
			.await;

		defmt::info!("waiting for PR...");
		if !super::svc_mqtt_config::CFG_PR_RUN.next().await {
			defmt::debug!("pr sent false; ignoring");
			continue;
		}

		defmt::info!("PR run started");

		bus.svc_leds
			.send(super::svc_leds::Cmd::SetState {
				state: super::svc_leds::State::PrPending,
			})
			.await;

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

		bus.svc_oled_pwr
			.send(super::svc_oled_pwr::Cmd::SetState {
				state: super::svc_oled_pwr::State::On,
			})
			.await;

		bus.dev_blinken_light
			.send(super::dev_blinken_light::Cmd::On)
			.await;

		bus.dev_leds
			.send(super::dev_leds::Cmd::SetJobIndicator(Rgb::new(245, 65, 5)))
			.await;

		// XXX
		Timer::after_secs(10).await;
	}
}
