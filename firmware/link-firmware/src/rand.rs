use embassy_stm32::{peripherals, rng};

static mut RNG: Option<rng::Rng<'static, peripherals::RNG>> = None;

pub fn init_rng(rng: rng::Rng<'static, peripherals::RNG>) {
	// SAFETY: Called once at startup
	#[expect(static_mut_refs)]
	unsafe {
		assert!(RNG.replace(rng).is_none());
	}
}

pub fn next_u64() -> u64 {
	// SAFETY: RNG is initialized at startup
	#[expect(static_mut_refs)]
	unsafe {
		RNG.as_mut().unwrap().next_u64()
	}
}

pub fn next_u32() -> u32 {
	// SAFETY: RNG is initialized at startup
	#[expect(static_mut_refs)]
	unsafe {
		RNG.as_mut().unwrap().next_u32()
	}
}
