#![no_std]
#![no_main]

mod arch;

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

#[no_mangle]
pub fn main() -> ! {
	let (mut dbgled, dbgserial, mut indlights) = unsafe { self::arch::Impl::initialize() };
	unsafe {
		DEBUG_WRITE = Some(dbgserial);
	}

	println!("Hello from {}!", "println");

	indlights.enable();

	const COLORS: [Color; 5] = [
		color::BLACK,
		color::WHITE,
		color::RED,
		color::GREEN,
		color::BLUE,
	];

	let mut color_idx = 0;

	loop {
		indlights.first(COLORS[color_idx % COLORS.len()]);
		indlights.second(COLORS[(color_idx + 1) % COLORS.len()]);
		indlights.third(COLORS[(color_idx + 2) % COLORS.len()]);
		color_idx = (color_idx + 1) % COLORS.len();

		dbgled.on();
		for _ in 0..1000000 {
			unsafe {
				::core::arch::asm!("NOP");
			}
		}
		dbgled.off();
		for _ in 0..1000000 {
			unsafe {
				::core::arch::asm!("NOP");
			}
		}
	}
}
