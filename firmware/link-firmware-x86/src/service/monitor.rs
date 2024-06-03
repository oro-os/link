use crate::{command::Command, uc::Monitor, CommandReceiver};
use defmt::warn;
use embassy_sync::{blocking_mutex::raw::RawMutex, mutex::Mutex};
use embassy_time::{Duration, Instant, Timer};

pub async fn run<M: Monitor, Rm: RawMutex, const SZ: usize>(
	monitor: &Mutex<Rm, M>,
	receiver: CommandReceiver<SZ>,
) -> ! {
	loop {
		{
			let mut monitor = monitor.lock().await;

			while let Ok(command) = receiver.try_receive() {
				match command {
					Command::SetScene(scene) => monitor.set_scene(scene),
					Command::Log(entry) => monitor.push_log(entry),
					// FIXME(qix-): disabled until I figure out why it's not working properly.
					//Command::SetStandby(standby) => monitor.standby_mode(standby),
					Command::StartTestSession {
						total_tests,
						author,
						title,
						ref_id,
					} => monitor.start_test_run(total_tests, author, title, ref_id),
					Command::StartTest { name } => monitor.start_test(name),
					unknown => warn!("monitor: ignoring unknown command: {:?}", unknown),
				}
			}

			let millis = Instant::now().as_millis();
			monitor.tick(millis);
		}

		Timer::after(Duration::from_millis(1000 / 60)).await;
	}
}
