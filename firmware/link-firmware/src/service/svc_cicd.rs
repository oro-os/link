use embassy_time::{Duration, Timer};

#[embassy_executor::task]
pub async fn run(_bus: super::Bus) -> ! {
	loop {
		Timer::after(Duration::from_secs(1000)).await;
	}
}
