#![no_std]
#![no_main]

mod arch;

use crate::arch::SystemUnderTest;

use self::arch::{color, Arch, Color, DebugLed, IndicatorLights};
use core::fmt::Write;
#[cfg(not(test))]
use core::panic::PanicInfo;

static mut DEBUG_WRITE: Option<<self::arch::Impl as Arch>::DebugSerialImpl> = None;

#[doc(hidden)]
pub fn _debug_print(args: ::core::fmt::Arguments) {
	if let Some(write) = unsafe { &mut DEBUG_WRITE } {
		write.write_fmt(args).unwrap();
	}
}

#[macro_export]
macro_rules! print {
	($($arg:tt)*) => ($crate::_debug_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
	() => ($crate::print!("\n"));
	($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[cfg(not(test))]
#[panic_handler]
fn panic(panic: &PanicInfo<'_>) -> ! {
	println!("PANIC: {:#?}", panic);
	loop {}
}

macro_rules! sleep_ticks {
	($n:literal) => {
		for _ in 0..$n {
			unsafe {
				::core::arch::asm!("NOP");
			}
		}
	};
}

#[no_mangle]
pub fn main() -> ! {
	let (mut dbgled, dbgserial, mut indlights, mut power_controller) =
		unsafe { self::arch::Impl::initialize() };
	unsafe {
		DEBUG_WRITE = Some(dbgserial);
	}

	println!(
		"Oro Link x86 rev6 firmware (version {})",
		env!("CARGO_PKG_VERSION")
	);
	println!("beginning POST:");

	dbgled.on();
	sleep_ticks!(500000);
	dbgled.off();
	println!("... debug led OK");

	indlights.enable();

	const COLORS: [Color; 8] = [
		color::BLACK,
		color::WHITE,
		color::RED,
		color::YELLOW,
		color::GREEN,
		color::CYAN,
		color::BLUE,
		color::MAGENTA,
	];

	for color_idx in 0..COLORS.len() {
		indlights.first(COLORS[color_idx % COLORS.len()]);
		indlights.second(COLORS[(color_idx + 1) % COLORS.len()]);
		indlights.third(COLORS[(color_idx + 2) % COLORS.len()]);
		sleep_ticks!(500000);
	}

	indlights.all_off();

	println!("... indicator lights OK");

	power_controller.set_power_state(arch::PowerState::Standby);
	println!("... psu standby OK");
	sleep_ticks!(10000000);
	power_controller.set_power_state(arch::PowerState::On);
	println!("... psu power OK");
	sleep_ticks!(10000000);
	power_controller.set_power_state(arch::PowerState::Off);
	println!("... psu OK");
	sleep_ticks!(10000000);

	println!("... ORO LINK OK");

	#[allow(clippy::empty_loop)] // XXX DEBUG
	loop {}
}
