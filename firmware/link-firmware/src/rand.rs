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

pub fn rng() -> impl rand_core::RngCore {
	struct FakeRng;

	#[expect(static_mut_refs)]
	impl rand_core::RngCore for FakeRng {
		fn fill_bytes(&mut self, dest: &mut [u8]) {
			// SAFETY: RNG is initialized at startup
			unsafe {
				RNG.as_mut().unwrap().fill_bytes(dest);
			}
		}

		fn next_u32(&mut self) -> u32 {
			// SAFETY: RNG is initialized at startup
			unsafe { RNG.as_mut().unwrap().next_u32() }
		}

		fn next_u64(&mut self) -> u64 {
			// SAFETY: RNG is initialized at startup
			unsafe { RNG.as_mut().unwrap().next_u64() }
		}
	}

	FakeRng
}
