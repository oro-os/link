use crate::uc::DebugLed;
use embassy_time::{Duration, Timer};

pub async fn run<L: DebugLed>(mut debug_led: L) {
	loop {
		debug_led.on();
		Timer::after(Duration::from_millis(100)).await;
		debug_led.off();
		Timer::after(Duration::from_millis(2000)).await;
	}
}
