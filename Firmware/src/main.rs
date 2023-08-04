#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

mod chip;
mod uc;

#[cfg(not(test))]
use core::panic::PanicInfo;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use uc::DebugLed;

#[cfg(not(test))]
#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
	loop {}
}

#[embassy_executor::main]
pub async fn main(_spawner: Spawner) {
	let (mut debug_led, _) = uc::init();

	loop {
		debug_led.on();
		Timer::after(Duration::from_millis(300)).await;

		debug_led.off();
		Timer::after(Duration::from_millis(300)).await;
	}
}
