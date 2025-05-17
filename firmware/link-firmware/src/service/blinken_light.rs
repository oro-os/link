use embassy_stm32::gpio::{Output, OutputOpenDrain};
use embassy_time::{Duration, Timer};

#[embassy_executor::task]
pub async fn blinken_light(mut light: OutputOpenDrain<'static>) -> ! {
	loop {
		light.set_low();
		Timer::after(Duration::from_millis(20)).await;
		light.set_high();
		Timer::after(Duration::from_millis(80)).await;
		light.set_low();
		Timer::after(Duration::from_millis(20)).await;
		light.set_high();
		Timer::after(Duration::from_millis(3000)).await;
	}
}
