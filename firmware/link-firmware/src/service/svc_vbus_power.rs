use core::cell::UnsafeCell;

use embassy_futures::select::{Either, select};
use embassy_stm32::{
	exti::ExtiInput,
	gpio::{Output, OutputOpenDrain},
	mode::Async,
};
use embassy_time::Timer;

use crate::{Stat, Volatile, nvram::LastBootFailure};

const AUX_VBUS_SWITCH_TIME_MS: u64 = 10;

pub static STAT_VBUS_STATE: Stat<State> = Stat::new("power/vbus_state");

pub type Channel = crate::channel::Channel<Cmd>;

pub struct Config {
	pub vbus_en:        Output<'static>,
	pub vbus_oc:        ExtiInput<'static, Async>,
	pub aux_vbus_en:    OutputOpenDrain<'static>,
	pub aux_vbus_sense: bool,
	pub failure:        &'static UnsafeCell<&'static mut Volatile<LastBootFailure>>,
}

#[derive(PartialEq, Eq)]
pub enum Cmd {
	Off,
	On,
}

#[derive(defmt::Format, Clone, Copy)]
pub enum State {
	Off,
	Vbus,
	AuxVbus,
}

impl AsRef<[u8]> for State {
	fn as_ref(&self) -> &[u8] {
		match self {
			State::Off => "off".as_bytes(),
			State::Vbus => "vbus".as_bytes(),
			State::AuxVbus => "aux_vbus".as_bytes(),
		}
	}
}

#[embassy_executor::task]
pub async fn run(rx: &'static Channel, config: Config) -> ! {
	let Config {
		mut vbus_en,
		mut vbus_oc,
		mut aux_vbus_en,
		failure,
		aux_vbus_sense,
	} = config;

	// vbus_en = active high
	// aux_vbus_en = active low

	let mut cmd = Cmd::Off;
	loop {
		// Sane reset.
		vbus_en.set_low();
		aux_vbus_en.set_high();

		cmd = match cmd {
			Cmd::Off => {
				defmt::debug!("turning off VBUS");
				STAT_VBUS_STATE.set(State::Off);
				// Already reset; continue;
				next_cmd(rx, cmd).await
			}
			Cmd::On => {
				defmt::debug!("turning on VBUS");
				let Either::First(next) = select(
					next_cmd(rx, cmd),
					run_vbus_driver(
						&mut vbus_en,
						&mut vbus_oc,
						&mut aux_vbus_en,
						aux_vbus_sense,
						failure,
					),
				)
				.await;
				next
			}
		};
	}
}

async fn run_vbus_driver(
	vbus_en: &mut Output<'static>,
	vbus_oc: &mut ExtiInput<'static, Async>,
	aux_vbus_en: &mut OutputOpenDrain<'static>,
	aux_vbus_sense: bool,
	failure: &'static UnsafeCell<&'static mut Volatile<LastBootFailure>>,
) -> ! {
	// Enable the VBUS line.
	defmt::debug!("enabling main vbus line");
	STAT_VBUS_STATE.set(State::Vbus);
	vbus_en.set_high();
	Timer::after_millis(10).await;

	// Wait for an OC event
	vbus_oc.wait_for_falling_edge().await;

	// Do we have an aux vbus?
	if !aux_vbus_sense {
		vbus_en.set_low();
		defmt::error!("main VBUS OC line asserted, but no AUX VBUS is sensed; panicking");

		// SAFETY: We're in a critical failure mode, resetting *is* the safe thing to do.
		// SAFETY: We can safely pull this value from the unsafecell since this is a blocking
		// SAFETY: call and the board is single-threaded. Thus, it's guaranteed that from the
		// SAFETY: time of this failure mode to board reset, nothing else will be able to take
		// SAFETY: a reference to the failure field.
		unsafe {
			failure.as_mut_unchecked().write(LastBootFailure::VbusOC);

			crate::reset();
		}
	}

	// Otherwise, enable the aux vbus line. Do this first,
	// wait 50ms for stabilization, and then kill off the
	// main vbus line so the board doesn't leech power.
	defmt::debug!("main VBUS OC line asserted; enabling aux power");
	aux_vbus_en.set_low();
	STAT_VBUS_STATE.set(State::AuxVbus);
	Timer::after_millis(AUX_VBUS_SWITCH_TIME_MS).await;
	defmt::debug!("aux VBUS power enabled; switching off main VBUS line");
	vbus_en.set_low();

	// Now the aux vbus failsafe service will take over if the aux vbus
	// line ever OC's. We can just halt here forever, since the
	// caller will cancel us if the power state changes.
	core::future::pending::<!>().await;
}

async fn next_cmd(rx: &'static Channel, current: Cmd) -> Cmd {
	loop {
		let new_command = rx.receive().await;
		if new_command != current {
			return new_command;
		}
	}
}
