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

#[expect(unused)]
pub fn rng() -> impl rand_core::Rng {
	struct FakeRng;

	#[expect(static_mut_refs)]
	impl rand_core::TryRng for FakeRng {
		type Error = rand_core::Infallible;

		fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
			// SAFETY: RNG is initialized at startup
			unsafe {
				RNG.as_mut().unwrap().fill_bytes(dest);
			}
			Ok(())
		}

		fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
			// SAFETY: RNG is initialized at startup
			Ok(unsafe { RNG.as_mut().unwrap().next_u32() })
		}

		fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
			// SAFETY: RNG is initialized at startup
			Ok(unsafe { RNG.as_mut().unwrap().next_u64() })
		}
	}

	FakeRng
}
