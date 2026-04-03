use embassy_time::{Duration, Timer};

#[embassy_executor::task]
pub async fn run(_bus: super::Bus) -> ! {
	Timer::after(Duration::from_secs(10)).await;
	defmt::warn!("about to go into WOL mode (5s)");
	Timer::after(Duration::from_secs(5)).await;
	unsafe { crate::wol::go_to_sleep_and_wait_for_wol() }
}
