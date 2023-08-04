#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

#[cfg(not(test))]
use core::panic::PanicInfo;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_time::{Duration, Timer};

#[cfg(not(test))]
#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
	loop {}
}

#[embassy_executor::main]
pub async fn main(_spawner: Spawner) {
	let p = embassy_stm32::init(Default::default());

	let mut led = Output::new(p.PE12, Level::Low, Speed::Low);

	loop {
		led.set_high();
		Timer::after(Duration::from_millis(300)).await;

		led.set_low();
		Timer::after(Duration::from_millis(300)).await;
	}
}
