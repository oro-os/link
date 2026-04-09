use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};

use super::dev_oled::Cmd as OledCmd;
use crate::service::svc_mqtt_stats::Stat;

pub type Channel = crate::channel::Channel<Cmd, 4>;

// const IDLE_WAIT_DURATION: Duration = Duration::from_secs(60 * 5); // 5 minutes
const IDLE_WAIT_DURATION: Duration = Duration::from_secs(10);
const IDLE_COOL_OFF_STEP_DURATION: Duration = Duration::from_millis(100);
const IDLE_COOL_OFF_STEP: u8 = 1; // Decrease brightness by 5 each step
const IDLE_MIN_BRIGHTNESS: u8 = 80; // Minimum brightness before vreg shutoff
const IDLE_VREG_OFF_DELAY: Duration = Duration::from_secs(10); // Time after turning off display to turn off VREG

pub static STAT_PWR_STATE: Stat<State> = Stat::new("power/oled/state");
pub static STAT_PWR_TARGET: Stat<State> = Stat::new("power/oled/target");

pub enum Cmd {
	SetState { state: State },
}

#[derive(defmt::Format, PartialEq, Eq, Copy, Clone)]
#[allow(unused)]
pub enum State {
	/// OLED should remain fully on
	On,
	/// OLED will slowly turn off after idle period
	Idle,
	/// OLED should immediately turn off
	Off,
}

impl AsRef<[u8]> for State {
	fn as_ref(&self) -> &[u8] {
		match self {
			State::On => "on".as_bytes(),
			State::Off => "off".as_bytes(),
			State::Idle => "idle".as_bytes(),
		}
	}
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, rx: &'static Channel) -> ! {
	let mut current_state = State::Off;

	STAT_PWR_STATE.set(current_state);
	STAT_PWR_TARGET.set(current_state);

	loop {
		let mut target_state = receive_state(rx, current_state).await;

		loop {
			defmt::debug!(
				"OLED power state change: {:?} -> {:?}",
				current_state,
				target_state
			);

			STAT_PWR_TARGET.set(target_state);

			let either = match target_state {
				State::On => select(receive_state(rx, target_state), perform_turnon(&bus)).await,
				State::Idle => {
					select(receive_state(rx, target_state), perform_idle_cooloff(&bus)).await
				}
				State::Off => select(receive_state(rx, target_state), perform_shutoff(&bus)).await,
			};

			current_state = target_state;
			STAT_PWR_STATE.set(current_state);

			if let Either::First(new_state) = either {
				// We were interrupted; start new target
				defmt::debug!(
					"OLED power state transition was interrupted with new state: {:?}",
					new_state
				);
				target_state = new_state;
			} else {
				// State transition completed successfully
				defmt::debug!("OLED power state transition completed successfully");
				break;
			}
		}
	}
}

async fn receive_state(rx: &'static Channel, ignore: State) -> State {
	loop {
		let Cmd::SetState { state } = rx.receive().await;
		if state != ignore {
			return state;
		}
	}
}

async fn perform_idle_cooloff(bus: &super::Bus) {
	// Transition to On if we aren't
	defmt::debug!("performing OLED idle cool-off; turning on display fully, first");
	perform_turnon(bus).await;

	// Wait for a few minutes of idle before turning off the display
	defmt::debug!(
		"OLED will begin cooldown after idle period: {:?}",
		IDLE_WAIT_DURATION
	);
	Timer::after(IDLE_WAIT_DURATION).await;

	// Gradually decrease brightness
	defmt::debug!("OLED idle cooldown starting");
	let mut brightness: u8 = 255;
	loop {
		if brightness <= IDLE_MIN_BRIGHTNESS {
			break;
		}

		brightness = brightness
			.saturating_sub(IDLE_COOL_OFF_STEP)
			.max(IDLE_MIN_BRIGHTNESS);
		defmt::debug!(
			"OLED idle cooldown step; setting brightness to {} and waiting for {:?}",
			brightness,
			IDLE_COOL_OFF_STEP_DURATION
		);
		bus.dev_oled
			.send(OledCmd::SetBrightness { brightness })
			.await;
		Timer::after(IDLE_COOL_OFF_STEP_DURATION).await;
	}

	// If we've reached here, it means there's no minimum brightness
	// and we can turn off the regulator
	defmt::debug!(
		"OLED idle cooldown has reached minimum brightness; turning off power after {:?}",
		IDLE_VREG_OFF_DELAY
	);
	Timer::after(IDLE_VREG_OFF_DELAY).await;

	defmt::debug!("OLED idle cooldown complete; shutting of OLED");
	bus.dev_oled
		.send(OledCmd::SetPower { enabled: false })
		.await;
}

async fn perform_shutoff(bus: &super::Bus) {
	// Immediately turn off display
	bus.dev_oled
		.send(OledCmd::SetBrightness { brightness: 0 })
		.await;

	// Wait a bit before turning off power regulator
	Timer::after(IDLE_VREG_OFF_DELAY).await;
	bus.dev_oled
		.send(OledCmd::SetPower { enabled: false })
		.await;
}

async fn perform_turnon(bus: &super::Bus) {
	bus.dev_oled.send(OledCmd::SetPower { enabled: true }).await;
	bus.dev_oled
		.send(OledCmd::SetBrightness { brightness: 255 })
		.await;
}
