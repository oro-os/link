use embassy_time::{Duration, Timer};

use crate::nvram::NvRamRebootStats;

#[embassy_executor::task]
pub async fn run(reboot: &'static mut NvRamRebootStats) {
	Timer::after(Duration::from_secs(5)).await;
	defmt::debug!("marking boot as successful");
	reboot.reset();
}
