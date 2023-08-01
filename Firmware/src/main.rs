#![no_std]
#![no_main]

mod arch;

use self::arch::{Arch, DebugLed};
#[cfg(not(test))]
use core::panic::PanicInfo;
use core::{fmt::Write, mem::MaybeUninit};

static mut DEBUG_WRITE: MaybeUninit<<self::arch::Impl as Arch>::DebugSerialImpl> =
	MaybeUninit::uninit();

#[doc(hidden)]
pub fn _debug_print(args: ::core::fmt::Arguments) {
	unsafe { DEBUG_WRITE.assume_init_mut() }
		.write_fmt(args)
		.unwrap();
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
fn panic(_panic: &PanicInfo<'_>) -> ! {
	loop {}
}

#[no_mangle]
pub fn main() -> ! {
	let (mut dbgled, dbgserial) = unsafe { self::arch::Impl::initialize() };
	unsafe {
		DEBUG_WRITE.write(dbgserial);
	}

	println!("Hello from {}!", "println");

	loop {
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
