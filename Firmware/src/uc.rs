pub mod helper;

#[cfg(feature = "stm32")]
mod stm32;
#[cfg(feature = "stm32")]
pub use stm32::*;

use core::cell::RefCell;
use embassy_executor::Spawner;
pub use embassy_net::driver::Driver as EthernetDriver;
use embassy_time::{block_for, Duration};
use embedded_io_async::{Read as AsyncRead, Write as AsyncWrite};
use heapless::String;
pub use rand_core::RngCore as Rng;

/// Controller for the MCU's debug LED, which is just a single LED used
/// to test basic I/O during POST and other states the firmware decides
/// to use it for.
pub trait DebugLed {
	/// Sets the LED on or off
	fn set_bit(&mut self, on: bool);

	/// Turns the LED on
	fn on(&mut self) {
		self.set_bit(true);
	}

	/// Turns the LED off
	fn off(&mut self) {
		self.set_bit(false);
	}
}

/// Power states in which a system under test can be.
/// The controller will properly transition between them,
/// which may block for a few hundred milliseconds.
#[allow(unused)]
#[derive(Debug, Clone, Copy)]
pub enum PowerState {
	/// Both the motherboard +5VSB line and PSU are off.
	Off,
	/// The motherboard +5bSB line is on, but the PSU is off.
	Standby,
	/// Both the motherboard +5VSB line and PSU are on.
	On,
}

/// Controller for the system under test (switches, CPU, etc.)
pub trait SystemUnderTest {
	/// Returns whether or not the SUT is requesting for the PSU
	/// to be turned on.
	fn power_requested(&self) -> bool;

	/// Returns the current power state of the SUT.
	fn current_state(&self) -> PowerState;

	/// Set the power state of the machine. Must transition between
	/// all combinations properly. This function MIGHT BLOCK the entire system.
	fn transition_power_state(&mut self, new_state: PowerState) {
		use PowerState as PS;

		match (self.current_state(), new_state) {
			(PS::Off, PS::Off) => { /* NO-OP */ }
			(PS::Off, PS::Standby) => {
				// Turn on the PSU standby
				unsafe {
					self.set_power_state(PS::Standby);
				}
				// Allow some time for the motherboard to come online
				block_for(Duration::from_millis(50));
			}
			(PS::Off, PS::On) => {
				// First transition to standby
				self.transition_power_state(PS::Standby);
				// Then transition to on
				self.transition_power_state(PS::On);
			}
			(PS::Standby, PS::Off) => {
				// Turn off the 5VSB pin
				unsafe {
					self.set_power_state(PS::Off);
				}
				// Allow motherboard to drain
				// ATX standard dictates no less than 16ms.
				block_for(Duration::from_millis(50));
			}
			(PS::Standby, PS::Standby) => { /* NO-OP */ }
			(PS::Standby, PS::On) => {
				// Turn on the PSU
				unsafe {
					self.set_power_state(PS::On);
				}
				// Wait for power to become stable
				// ATX standard dictates no less than 100ms.
				block_for(Duration::from_millis(150));
			}
			(PS::On, PS::Off) => {
				// First transition to standby
				self.transition_power_state(PS::Standby);
				// Then transition to off
				self.transition_power_state(PS::Off);
			}
			(PS::On, PS::Standby) => {
				// Turn off the PSU
				unsafe {
					self.set_power_state(PS::Standby);
				}
				// Give the PSU a little breathing room.
				block_for(Duration::from_millis(50));
			}
			(PS::On, PS::On) => { /* NO-OP */ }
		}
	}

	/// Immediately set the power state of the machine.
	///
	/// # Safety
	///
	/// No transitioning is done; calling this function too quickly
	/// with different states *may* cause system instability or, perhaps,
	/// even damage in the extreme edge case. Use it cautiously, and with
	/// sufficient delays in between.
	///
	/// You should probably use `transition_power_state` instead.
	unsafe fn set_power_state(&mut self, new_state: PowerState);

	/// Triggers a reset of the machine (via the reset switch)
	/// with a pulse length of 100ms.
	fn reset(&mut self) {
		self.reset_ms(100);
	}

	/// Triggers a reset of the machine (via the reset switch),
	/// holding it for a specific number of milliseconds.
	fn reset_ms(&mut self, ms: u64);

	/// Triggers an ACPI power signal to the machine (via the power switch)
	/// with a pulse length of 100ms.
	fn power(&mut self) {
		self.power_ms(100);
	}

	/// Triggers an ACPI power signal to the machine (via the power switch),
	/// holding it for a specific number of milliseconds.
	fn power_ms(&mut self, ms: u64);
}

/// A singular mode that a monitor should be in.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
	/// Displays the Oro logo and any aesthetically pleasing effect on
	/// LEDs, etc.
	OroLogo,
	/// Displays a running log of diagnostic frames.
	#[default]
	Log,
	/// Displays the status of a test run.
	Test,
}

/// A log frame's severity level.
/// All log frames are considered important if the firmware pushes them;
/// implementations of [`Monitor`] should not perform any filtering.
#[allow(unused)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LogSeverity {
	Info,
	Warn,
	Error,
}

impl LogSeverity {
	pub fn log<M: Monitor>(self, monitor: &RefCell<M>, message: String<256>) {
		let mut monitor = monitor.borrow_mut();
		monitor.push_log(LogFrame {
			severity: self,
			message,
		});
	}
}

/// A singular log frame.
/// All log frames are considered important if the firmware pushes them;
/// implementations of [`Monitor`] should not perform any filtering.
///
/// Log lines that are too long should split on the nearest whitespace.
pub struct LogFrame {
	pub severity: LogSeverity,
	pub message: String<256>,
}

/// The monitor/screen and indicators for monitoring the status of the
/// system. This is a **very high level** API for screens and indicators
/// lights as it's needed by the firmware, thus providing the platform-specific
/// hardware implementators to change how status is displayed to the user.
pub trait Monitor {
	/// Enable/disable standby mode. When standby mode is enabled,
	/// the firmware is indicating that nothing is happening at that specific moment,
	/// and that the monitor should (eventually) turn off to conserve power/reduce
	/// light output. Standby **does not** have to have an immediate effect; for example,
	/// the LEDs/OLED/etc. may fade out over a long period of time.
	fn standby_mode(&mut self, enable: bool);

	/// Switches the scene of the monitor. A 'scene' is a whole-frame mode
	/// in which certain information is shown. If the same scene as the current
	/// scene is passed, nothing should happen (do **not** restart the scene, for example).
	fn set_scene(&mut self, scene: Scene);

	/// Consumes a log frame. If the current monitor scene is not [`Scene::Log`],
	/// the log frame should be stored for later display. Log frames that would otherwise
	/// go out of frame can be dropped and forgotten about.
	///
	/// Firmware only pushes important log frames; implementors of this trait should not
	/// peform any filtering themselves.
	fn push_log(&mut self, frame: LogFrame);

	/// Starts a test run with the given number of tests.
	/// Every successive call to "start_test" will decrement that count by one
	/// in order to show progress.
	///
	/// NOTE: This does NOT change the scene!
	fn start_test_run(
		&mut self,
		total: usize,
		author: String<256>,
		title: String<256>,
		ref_id: String<256>,
	);

	/// Indicates the start of a new test
	fn start_test(&mut self, name: String<256>);

	/// Should be called frequently - at least 60 times a second, but can be called
	/// faster. Must be passed a monotonic millisecond instance.
	fn tick(&mut self, millis: u64);
}

/// A real-world date/time
#[derive(Default, Clone, defmt::Format)]
pub struct DateTime {
	pub year: u16,
	/// 1 is January
	pub month: u8,
	/// 1 is the first day
	pub day: u8,
	/// 0 is Sunday, 1 is Monday
	pub day_of_week: u8,
	pub hour: u8,
	pub minute: u8,
	pub second: u8,
	/// Whether or not we're observing DST
	pub dst: bool,
}

/// Implements a wall clock (RTC)
pub trait WallClock {
	/// Sets the current date/time
	fn set_datetime(&mut self, dt: DateTime);

	/// Gets the current date/time
	fn get_datetime(&self) -> Option<DateTime>;
}

/// The writing end of the system's Uart
pub trait UartTx: AsyncWrite {}
/// The reading end of the system's Uart
pub trait UartRx: AsyncRead {}

impl<T> UartTx for T where T: AsyncWrite {}
impl<T> UartRx for T where T: AsyncRead {}

/// Writes packets to a peripheral for use with a PCAP-like daemon.
pub trait PacketTracer {
	/// Trace a packet. Sends the u16be length and then the packet
	/// contents over the transport medium.
	///
	/// # Panics
	/// If `buf` is larger than 65535 (`u16::MAX`) bytes long, or
	/// if a communication error occurred and couldn't be recovered from.
	fn trace_packet(&mut self, buf: &[u8]);
}

// Validates the contract of the init() function.
#[allow(unused)]
#[doc(hidden)]
mod _check_init {
	use super::*;
	trait Init {
		fn ok(self)
		where
			Self: Sized,
		{
		}
	}

	macro_rules! fake_ref {
		($ty:ty) => {
			#[allow(deref_nullptr, clippy::zero_ptr)]
			&*(0x0 as *const $ty)
		};
	}

	async unsafe fn _check_init() {
		Init::ok(init(fake_ref![Spawner]).await)
	}

	impl<
		DBG: DebugLed,
		SUT: SystemUnderTest,
		MON: Monitor,
		EXTETH: EthernetDriver,
		SYSETH: EthernetDriver,
		CLOCK: WallClock,
		RNG: Rng,
		USARTTX: UartTx,
		USARTRX: UartRx,
		PKTTRACER: PacketTracer,
	> Init
		for (
			DBG,
			SUT,
			MON,
			EXTETH,
			SYSETH,
			CLOCK,
			RNG,
			USARTTX,
			USARTRX,
			PKTTRACER,
		)
	{
	}
}
