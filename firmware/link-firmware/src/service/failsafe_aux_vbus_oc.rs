//! Failsafe that detects over-current events for aux VBUS
//! line when switched on.

use core::cell::UnsafeCell;

use embassy_stm32::{exti::ExtiInput, mode::Async};

use crate::{Volatile, nvram::LastBootFailure};

pub struct Config {
	pub aux_vbus_oc: ExtiInput<'static, Async>,
	pub failure:     &'static UnsafeCell<&'static mut Volatile<LastBootFailure>>,
}

#[embassy_executor::task]
pub async fn run(config: Config) -> ! {
	let Config {
		mut aux_vbus_oc,
		failure,
	} = config;

	aux_vbus_oc.wait_for_low().await;
	defmt::error!("aux VBUS OC line asserted; resetting");
	// SAFETY: We're in a critical failure mode, resetting *is* the safe thing to do.
	// SAFETY: We can safely pull this value from the unsafecell since this is a blocking
	// SAFETY: call and the board is single-threaded. Thus, it's guaranteed that from the
	// SAFETY: time of this failure mode to board reset, nothing else will be able to take
	// SAFETY: a reference to the failure field.
	unsafe {
		failure.as_mut_unchecked().write(LastBootFailure::AuxVbusOC);

		crate::reset();
	}
}
