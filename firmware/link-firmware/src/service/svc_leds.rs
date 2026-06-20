use embassy_futures::select::{Either, select};

use crate::color::Rgb;

pub type Channel = crate::channel::Channel<Cmd, 4>;

pub enum Cmd {
	SetState { state: State },
}

#[derive(defmt::Format, Clone, Copy, PartialEq, Eq)]
pub enum State {
	Off,
	PrPending,
}

impl From<State> for heapless::String<16> {
	fn from(state: State) -> Self {
		match state {
			State::Off => "off".try_into().unwrap(),
			State::PrPending => "pr_pending".try_into().unwrap(),
		}
	}
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, rx: &'static Channel) -> ! {
	let mut current_state = State::Off;

	crate::vars::STAT_LEDS_STATE.set(current_state.into()).await;
	crate::vars::STAT_LEDS_TARGET_STATE
		.set(current_state.into())
		.await;

	loop {
		let mut target_state = receive_state(rx, current_state).await;

		loop {
			defmt::debug!(
				"LEDS state change: {:?} -> {:?}",
				current_state,
				target_state
			);

			crate::vars::STAT_LEDS_TARGET_STATE
				.set(target_state.into())
				.await;

			let either = match target_state {
				State::Off => select(receive_state(rx, target_state), perform_turnoff(&bus)).await,
				State::PrPending => {
					select(receive_state(rx, target_state), perform_pr_pending(&bus)).await
				}
			};

			current_state = target_state;
			crate::vars::STAT_LEDS_STATE.set(current_state.into()).await;

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
