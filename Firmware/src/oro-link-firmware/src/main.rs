use crate::arch::Arch;

static mut DEBUG_WRITE: Option<fn(&str)> = None;

pub fn main<A: Arch>() {
	unsafe {
		DEBUG_WRITE = Some(A::debug_write);
	}

	unsafe { DEBUG_WRITE }.unwrap()("Hello, Oro main!\n");
}
