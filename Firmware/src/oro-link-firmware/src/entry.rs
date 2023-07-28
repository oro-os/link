use crate::arch::Arch;

static mut DEBUG_WRITE: Option<fn(::core::fmt::Arguments)> = None;

#[doc(hidden)]
pub fn _print(args: ::core::fmt::Arguments) {
	unsafe { DEBUG_WRITE }.as_ref().unwrap()(args);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => (crate::entry::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

pub fn main<A: Arch>() {
	unsafe {
		DEBUG_WRITE = Some(A::debug_write);
	}

	println!("Hello, Oro {}!", "println");
}
