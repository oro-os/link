//! Failsafe that detects over-current events for the 5V main
//! board power via the power monitor.

use core::cell::UnsafeCell;

use embassy_stm32::{exti::ExtiInput, mode::Async};
use embassy_time::Timer;

use crate::{Volatile, nvram::LastBootFailure};

pub const ALERT_ON_CURRENT_MA: u16 = 1900;

pub struct Config {
	pub board_power_alert: ExtiInput<'static, Async>,
	pub failure:           &'static UnsafeCell<&'static mut Volatile<LastBootFailure>>,
}

#[embassy_executor::task]
pub async fn run(config: Config) -> ! {
	let Config {
		mut board_power_alert,
		failure,
	} = config;

	// Wait a moment to let any current in-rush to pass,
	// and to let the power monitor service reset the chip
	// (and bring low high the alert pin).
	Timer::after_millis(100).await;

	crate::vars::STAT_BOARD_OC_MA.set(ALERT_ON_CURRENT_MA as i64);

	board_power_alert.wait_for_low().await;
	defmt::error!("board power OC alert; rebooting");
	// SAFETY: We're in a critical failure mode, resetting *is* the safe thing to do.
	// SAFETY: We can safely pull this value from the unsafecell since this is a blocking
	// SAFETY: call and the board is single-threaded. Thus, it's guaranteed that from the
	// SAFETY: time of this failure mode to board reset, nothing else will be able to take
	// SAFETY: a reference to the failure field.
	unsafe {
		failure
			.as_mut_unchecked()
			.write(LastBootFailure::PowerMonitorOC);

		crate::reset();
	}
}
