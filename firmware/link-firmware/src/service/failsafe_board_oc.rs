//! Failsafe that detects over-current events for the 3V3 regulator
//! via the power monitor.

use embassy_stm32::{exti::ExtiInput, mode::Async};
use embassy_time::Timer;

use crate::service::svc_mqtt_stats::StrStat;

pub const ALERT_ON_CURRENT_MA: u16 = 1900;

pub static STAT_OC_MA: StrStat<u16, 7> = StrStat::new("power/oc_limit_board");

pub struct Config {
	pub board_power_alert: ExtiInput<'static, Async>,
}

#[embassy_executor::task]
pub async fn run(config: Config) -> ! {
	let Config {
		mut board_power_alert,
	} = config;

	// Wait a moment to let any current in-rush to pass,
	// and to let the power monitor service reset the chip
	// (and bring low high the alert pin).
	Timer::after_millis(100).await;

	STAT_OC_MA.set(ALERT_ON_CURRENT_MA);

	board_power_alert.wait_for_low().await;
	panic!("board power OC alert");
}
