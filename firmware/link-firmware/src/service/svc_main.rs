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
	crate::bus!(bus, dev_blinken_light, Config);
	crate::bus!(
		bus,
		svc_oled_pwr,
		SetState {
			state: super::svc_oled_pwr::State::On,
		}
	);
	crate::bus!(
		bus,
		svc_oled,
		SetScene {
			scene: super::svc_oled::Scene::Logo,
		}
	);

	// Perform LED self-test
	crate::bus!(bus, dev_leds, SelfTest);

	// Wait for self-test to complete (and let the logo show up :D)
	Timer::after_secs(5).await;

	// Wait for MQTT to connect
	crate::bus!(bus, dev_leds, AllOff);
	crate::bus!(bus, dev_leds, SetSystemIndicator(Rgb::new(245, 65, 5)));
	crate::bus!(bus, dev_leds, SetRemoteIndicator(Rgb::new(245, 65, 5)));

	crate::oled_status!(bus, Bold("Waiting for MQTT..."), Normal(crate::unique_id()),);

	mqtt.get().await;
	crate::bus!(bus, dev_leds, SetRemoteIndicator(Rgb::new(2, 18, 2)));

	// Wait for global configuration
	macro_rules! fetch_pr_config {
		($cfg:expr) => {{
			crate::oled_status!(
				bus,
				Bold("Fetching configuration..."),
				Normal($cfg.suffix()),
			);
			let v = $cfg.next().await;
			defmt::info!("Global config fetched: {} = {:?}", $cfg.suffix(), v);
			v
		}};
	}

	let _power_type = fetch_pr_config!(super::svc_mqtt_config::CFG_GLOBAL_POWER_TYPE);
	let _usb_iface = fetch_pr_config!(super::svc_mqtt_config::CFG_GLOBAL_USB_IFACE);
	let _boot_source = fetch_pr_config!(super::svc_mqtt_config::CFG_GLOBAL_BOOT_SOURCE);
	let _require_4a_vbus = fetch_pr_config!(super::svc_mqtt_config::CFG_GLOBAL_REQUIRE_4A_VBUS);
	let _wol = fetch_pr_config!(super::svc_mqtt_config::CFG_GLOBAL_WOL);

	crate::bus!(bus, dev_leds, SetSystemIndicator(Rgb::new(2, 18, 2)));
	crate::bus!(bus, dev_blinken_light, Idle);

	// SAFETY: keep the number of seconds in the single digits.
	#[expect(static_mut_refs)]
	for s in (1..=5).rev() {
		static mut TIMEOUT_MSG: heapless::String<24> = heapless::String::new();
		// SAFETY: this is the only place it's used and it's used in lockstep;
		// SAFETY: technically UB but not a problem in this very specific case.
		// SAFETY: if the seconds is ever increased to double digits, a race
		// SAFETY: condition could occur, so don't do that.
		unsafe {
			TIMEOUT_MSG = heapless::format!("standing by in {s}s...").unwrap();
		}

		crate::oled_status!(
			bus,
			Bold("MQTT connected"),
			Normal(unsafe { TIMEOUT_MSG.as_str() })
		);

		Timer::after_secs(1).await;
	}

	loop {
		crate::bus!(bus, dev_leds, SetJobIndicator(Rgb::new(2, 1, 1)));
		crate::bus!(
			bus,
			svc_oled,
			SetScene {
				scene: super::svc_oled::Scene::Logo,
			}
		);
		crate::bus!(
			bus,
			svc_oled_pwr,
			SetState {
				state: super::svc_oled_pwr::State::Idle,
			}
		);
		crate::bus!(
			bus,
			svc_leds,
			SetState {
				state: super::svc_leds::State::Off,
			}
		);
		crate::bus!(bus, dev_blinken_light, Idle);

		defmt::info!("waiting for PR...");
		if !super::svc_mqtt_config::CFG_PR_RUN.next().await {
			defmt::debug!("pr sent false; ignoring");
			continue;
		}

		defmt::info!("PR run started");

		crate::bus!(
			bus,
			svc_leds,
			SetState {
				state: super::svc_leds::State::PrPending,
			}
		);

		crate::oled_status!(bus, Bold("PR started; fetching configuration"), Normal(""),);
		Timer::after_millis(10).await;

		crate::bus!(
			bus,
			svc_oled_pwr,
			SetState {
				state: super::svc_oled_pwr::State::On,
			}
		);
		Timer::after_millis(10).await;

		crate::bus!(bus, dev_blinken_light, On);
		crate::bus!(bus, dev_leds, SetJobIndicator(Rgb::new(245, 65, 5)));

		macro_rules! fetch_pr_config {
			($cfg:expr) => {{
				crate::oled_status!(
					bus,
					Bold("PR started; fetching configuration"),
					Normal($cfg.suffix()),
				);
				let v = $cfg.next().await;
				defmt::info!("PR config fetched: {} = {}", $cfg.suffix(), v);
				v
			}};
		}

		let _pr_title = fetch_pr_config!(super::svc_mqtt_config::CFG_PR_TITLE);
		let _pr_number = fetch_pr_config!(super::svc_mqtt_config::CFG_PR_NUMBER);
		let _pr_author = fetch_pr_config!(super::svc_mqtt_config::CFG_PR_AUTHOR);

		crate::oled_status!(
			bus,
			Bold("PR started; fetching image"),
			Normal("connecting...")
		);

		// XXX
		Timer::after_secs(10).await;
	}
}
