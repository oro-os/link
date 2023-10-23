use crate::{command::Command, uc::Monitor, CommandReceiver};
use core::cell::RefCell;
use defmt::warn;
use embassy_time::{Duration, Instant, Timer};

pub async fn run<M: Monitor, const SZ: usize>(
	monitor: &RefCell<M>,
	receiver: CommandReceiver<SZ>,
) -> ! {
	loop {
		if let Ok(mut monitor) = monitor.try_borrow_mut() {
			while let Ok(command) = receiver.try_receive() {
				match command {
					Command::SetScene(scene) => monitor.set_scene(scene),
					Command::Log(entry) => monitor.push_log(entry),
					Command::SetStandby(standby) => monitor.standby_mode(standby),
					Command::StartTestSession {
						total_tests,
						author,
						title,
						ref_id,
					} => monitor.start_test_run(total_tests, author, title, ref_id),
					unknown => warn!("monitor: ignoring unknown command: {:?}", unknown),
				}
			}

			let millis = Instant::now().as_millis();
			monitor.tick(millis);
		}

		Timer::after(Duration::from_millis(1000 / 240)).await;
	}
}
