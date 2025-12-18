use embassy_stm32::gpio::OutputOpenDrain;
use embassy_time::{Duration, Timer};

#[embassy_executor::task]
pub async fn blinken_light(
	mut debug_led1: OutputOpenDrain<'static>,
	mut debug_led2: OutputOpenDrain<'static>,
	mut debug_led3: OutputOpenDrain<'static>,
) -> ! {
	loop {
		for led in [&mut debug_led1, &mut debug_led2, &mut debug_led3] {
			for _ in 0..2 {
				led.set_low();
				Timer::after(Duration::from_millis(10)).await;
				led.set_high();
				Timer::after(Duration::from_millis(20)).await;
			}
			Timer::after(Duration::from_millis(30)).await;
		}
	}
}
