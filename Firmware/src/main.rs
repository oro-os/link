#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

mod chip;
mod uc;

#[cfg(not(test))]
use core::panic::PanicInfo;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use uc::{color, DebugLed, IndicatorLights};

#[cfg(not(test))]
#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
	loop {}
}

#[embassy_executor::main]
pub async fn main(_spawner: Spawner) {
	let (mut debug_led, _, mut indicators) = uc::init();

	indicators.enable();
	indicators.first(color::RED);
	indicators.second(color::GREEN);
	indicators.third(color::BLUE);

	loop {
		debug_led.on();
		Timer::after(Duration::from_millis(300)).await;

		debug_led.off();
		Timer::after(Duration::from_millis(3000)).await;
	}
}
