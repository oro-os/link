#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

mod chip;
mod uc;

#[cfg(not(test))]
use core::panic::PanicInfo;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use uc::{DebugLed, PowerState, SystemUnderTest};

#[cfg(not(test))]
#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
	loop {}
}

#[embassy_executor::main]
pub async fn main(_spawner: Spawner) {
	let (mut debug_led, mut system) = uc::init();

	Timer::after(Duration::from_millis(1000)).await;
	system.transition_power_state(PowerState::Standby);
	Timer::after(Duration::from_millis(2000)).await;
	system.transition_power_state(PowerState::On);
	Timer::after(Duration::from_millis(3000)).await;
	system.power();
	Timer::after(Duration::from_millis(1000)).await;
	system.reset();
	Timer::after(Duration::from_millis(3000)).await;

	loop {
		debug_led.on();
		Timer::after(Duration::from_millis(300)).await;

		system.reset();

		debug_led.off();
		Timer::after(Duration::from_millis(7000)).await;
	}
}
