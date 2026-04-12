use embassy_futures::join::join;
use embassy_time::{Duration, Timer};

pub type Channel = crate::channel::Channel<Cmd, 2>;

pub enum Cmd {
	Off,
	After(Duration),
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, rx: &'static Channel) -> ! {
	let mut cmd = Cmd::Off;
	loop {
		(cmd, _) = join(rx.receive(), run_wol_watcher(&bus, cmd)).await;
	}
}

#[expect(static_mut_refs)]
async fn run_wol_watcher(bus: &super::Bus, cmd: Cmd) {
	let duration = match cmd {
		Cmd::Off => {
			defmt::debug!("WoL: timer off");
			return;
		}
		Cmd::After(d) => d,
	};

	// Failsafe; calling WOL too quickly causes the board to become unbootable.
	if duration.as_secs() < 10 {
		panic!("refusing to go into WoL in a shorter amount of time than 10s");
	}

	defmt::debug!("WoL: will sleep after {} seconds", duration.as_secs());
	Timer::after(duration).await;

	crate::bus!(
		bus,
		svc_oled_pwr,
		SetState {
			state: super::svc_oled_pwr::State::On,
		}
	);

	for countdown in (1..=10).rev() {
		static mut COUNTDOWN: heapless::String<10> = heapless::String::new();

		// SAFETY: Safe here, we're the only ones using it.
		unsafe {
			COUNTDOWN = heapless::format!("in {countdown}s..").unwrap();
		}

		defmt::warn!("link going into WoL in {} seconds", countdown);

		crate::oled_status!(
			bus,
			Normal("Oro Link going into wake-on-lan"),
			// SAFETY: Technically UB but should be fine.
			Normal(unsafe { COUNTDOWN.as_str() })
		);

		Timer::after_secs(1).await;
	}

	defmt::warn!("link going into WoL");
	// SAFETY: the only place we care about going into wol
	unsafe { crate::wol::go_to_sleep_and_wait_for_wol() }
}
