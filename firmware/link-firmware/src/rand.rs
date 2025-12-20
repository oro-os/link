use embassy_stm32::{peripherals, rng};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};

static mut RNG: Option<Mutex<NoopRawMutex, rng::Rng<'static, peripherals::RNG>>> = None;

pub fn init_rng(rng: rng::Rng<'static, peripherals::RNG>) {
	// SAFETY: Called once at startup
	#[expect(static_mut_refs)]
	unsafe {
		assert!(RNG.replace(Mutex::new(rng)).is_none());
	}
}

pub async fn next_u64() -> u64 {
	// SAFETY: RNG is initialized at startup
	#[expect(static_mut_refs)]
	unsafe {
		RNG.as_ref().unwrap().lock().await.next_u64()
	}
}
