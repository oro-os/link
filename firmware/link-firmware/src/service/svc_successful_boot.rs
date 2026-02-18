use embassy_time::{Duration, Timer};

use crate::nvram::NvRamRebootStats;

pub struct Config {
	pub reboot: &'static mut NvRamRebootStats,
}

#[embassy_executor::task]
pub async fn run(config: Config) {
	let Config { reboot } = config;
	Timer::after(Duration::from_secs(5)).await;
	defmt::debug!("marking boot as successful");
	reboot.reset();
}
