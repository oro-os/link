//! Failsafe that detects over-current events ULPI USB VBUS line.
//!
//! **WARNING:** Largely untested, as the link has had no host mode
//! firmware testing whatsoever.

use core::cell::UnsafeCell;

use embassy_stm32::{exti::ExtiInput, mode::Async};

use crate::{Volatile, nvram::LastBootFailure};

pub struct Config {
	pub ulpi_oc: ExtiInput<'static, Async>,
	pub failure: &'static UnsafeCell<&'static mut Volatile<LastBootFailure>>,
}

#[embassy_executor::task]
pub async fn run(config: Config) -> ! {
	let Config {
		mut ulpi_oc,
		failure,
	} = config;

	ulpi_oc.wait_for_low().await;
	defmt::error!("ULPI OC event");
	// SAFETY: We're in a critical failure mode, resetting *is* the safe thing to do.
	// SAFETY: We can safely pull this value from the unsafecell since this is a blocking
	// SAFETY: call and the board is single-threaded. Thus, it's guaranteed that from the
	// SAFETY: time of this failure mode to board reset, nothing else will be able to take
	// SAFETY: a reference to the failure field.
	unsafe {
		failure.as_mut_unchecked().write(LastBootFailure::UlpiOC);

		crate::reset();
	}
}
