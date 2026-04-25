use core::future;

use embassy_stm32::gpio::Output;
use embassy_time::{Duration, Timer};

use crate::{color::Rgb, nvram::LastBootFailure};

pub struct Config {
	pub aux_vbus_sense:      bool,
	pub last_boot_failure:   LastBootFailure,
	pub usb_output_selector: Output<'static>,
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, config: Config) -> ! {
	let Config {
		aux_vbus_sense,
		last_boot_failure,
		mut usb_output_selector,
	} = config;

	// Was the last boot a failure?
	if last_boot_failure != LastBootFailure::None {
		crate::bus!(bus, dev_blinken_light, Error);
		crate::bus!(bus, dev_leds, SetSystemIndicator(Rgb::new(255, 0, 0)));
		crate::bus!(
			bus,
			svc_oled_pwr,
			SetState {
				state: super::svc_oled_pwr::State::On,
			}
		);
		crate::oled_status!(
			bus,
			Bold("!!  CRITICAL FAILURE  !!"),
			Normal(last_boot_failure.as_str()),
			Normal(""),
			Normal("(board must be manually reset)")
		);
		future::pending::<!>().await; // never continue
	}

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

	// Wait for QUP to connect
	crate::bus!(bus, dev_leds, AllOff);
	crate::bus!(bus, dev_leds, SetSystemIndicator(Rgb::new(245, 65, 5)));
	crate::bus!(bus, dev_leds, SetRemoteIndicator(Rgb::new(245, 65, 5)));

	crate::oled_status!(
		bus,
		Bold("Waiting for config..."),
		Normal(crate::unique_id()),
	);
	crate::vars::CFG_CONFIGURED.wait_for(&true).await;

	crate::bus!(bus, dev_leds, SetRemoteIndicator(Rgb::new(2, 18, 2)));
	crate::oled_status!(bus, Bold("Configuring..."), Normal(crate::unique_id()),);

	let _power_type = crate::vars::CFG_SUT_POWER_TYPE.get();
	let usb_iface = crate::vars::CFG_SUT_USB_IFACE.get();
	let require_4a_vbus = crate::vars::CFG_SUT_REQUIRE_4A_VBUS.get();
	let _boot_source = crate::vars::CFG_SUT_BOOT_SOURCE.get();
	let wol = crate::vars::CFG_WOL.get();

	// Set the USB output selector
	match usb_iface {
		crate::vars::UsbIface::Header => {
			usb_output_selector.set_low();
			defmt::debug!("USB is routed to header");
		}
		crate::vars::UsbIface::Port => {
			usb_output_selector.set_high();
			defmt::debug!("USB is routed to port");
		}
	}

	// Make sure 4A VBUS line is sensed if needed.
	// Note that the sense line is active-low, so if it's high, it means there's no
	// aux VBUS line.
	defmt::debug!(
		"checking 4A vbus requirements: required = {}, aux_vbus_sense = {}",
		require_4a_vbus,
		aux_vbus_sense
	);
	if require_4a_vbus && !aux_vbus_sense {
		crate::bus!(bus, dev_leds, SetSystemIndicator(Rgb::new(255, 0, 0)));

		crate::oled_status!(
			bus,
			Bold("4A VBUS line required"),
			Normal("Connect a 4A-equipped power supply"),
			Normal("and reboot the Link"),
		);

		crate::bus!(bus, dev_blinken_light, Error);

		future::pending::<!>().await; // never continue
	}

	// Signal all good
	crate::bus!(bus, dev_leds, SetSystemIndicator(Rgb::new(2, 18, 2)));
	crate::bus!(bus, dev_blinken_light, Idle);

	loop {
		// Reset state
		crate::vars::CFG_PR_RUN.set(false);
		crate::vars::CFG_PR_TITLE.set(Default::default());
		crate::vars::CFG_PR_AUTHOR.set(Default::default());
		crate::vars::CFG_PR_NUMBER.set(0);

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
		crate::bus!(bus, svc_vbus_power, Off);
		crate::bus!(bus, svc_psu, Off);

		match wol {
			crate::vars::Wol::Off => crate::bus!(bus, svc_wol, Off),
			crate::vars::Wol::Mins5 => {
				crate::bus!(bus, svc_wol, After(Duration::from_secs(5 * 60)))
			}
			crate::vars::Wol::Mins10 => {
				crate::bus!(bus, svc_wol, After(Duration::from_secs(10 * 60)))
			}
			crate::vars::Wol::Mins30 => {
				crate::bus!(bus, svc_wol, After(Duration::from_secs(30 * 60)))
			}
		}

		defmt::info!("waiting for PR...");
		crate::vars::CFG_PR_RUN.wait_for(&true).await;
		defmt::info!("PR run started");

		crate::bus!(bus, svc_wol, Off);

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

		let _pr_title = crate::vars::CFG_PR_TITLE.get();
		let _pr_number = crate::vars::CFG_PR_NUMBER.get();
		let _pr_author = crate::vars::CFG_PR_AUTHOR.get();

		crate::oled_status!(
			bus,
			Bold("PR started; fetching image"),
			Normal("connecting...")
		);

		// XXX
		crate::bus!(bus, svc_vbus_power, On);
		Timer::after_secs(30).await;
	}
}
