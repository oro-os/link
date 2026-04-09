use embassy_futures::select::{Either, select};

use crate::{color::Rgb, service::svc_mqtt_stats::Stat};

pub type Channel = crate::channel::Channel<Cmd, 4>;

pub static STAT_STATE: Stat<State> = Stat::new("status/leds/state");
pub static STAT_TARGET: Stat<State> = Stat::new("status/leds/target");
pub enum Cmd {
	SetState { state: State },
}

#[derive(defmt::Format, Clone, Copy, PartialEq, Eq)]
pub enum State {
	Off,
	PrPending,
}

impl AsRef<[u8]> for State {
	fn as_ref(&self) -> &[u8] {
		match self {
			State::Off => "off".as_bytes(),
			State::PrPending => "pr_pending".as_bytes(),
		}
	}
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, rx: &'static Channel) -> ! {
	let mut current_state = State::Off;

	STAT_STATE.set(current_state);
	STAT_TARGET.set(current_state);

	loop {
		let mut target_state = receive_state(rx, current_state).await;

		loop {
			defmt::debug!(
				"LEDS state change: {:?} -> {:?}",
				current_state,
				target_state
			);

			STAT_TARGET.set(target_state);

			let either = match target_state {
				State::Off => select(receive_state(rx, target_state), perform_turnoff(&bus)).await,
				State::PrPending => {
					select(receive_state(rx, target_state), perform_pr_pending(&bus)).await
				}
			};

			current_state = target_state;
			STAT_STATE.set(current_state);

			if let Either::First(new_state) = either {
				// We were interrupted; start new target
				defmt::debug!(
					"LEDS state transition was interrupted with new state: {:?}",
					new_state
				);
				target_state = new_state;
			} else {
				// State transition completed successfully
				defmt::debug!("LEDS state transition completed successfully");
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

async fn perform_turnoff(bus: &super::Bus) {
	bus.dev_leds.send(super::dev_leds::Cmd::AllOff).await;
}

async fn perform_pr_pending(bus: &super::Bus) {
	bus.dev_leds
		.send(super::dev_leds::Cmd::SetBacklight(Rgb::grey(0xFF)))
		.await;
}
