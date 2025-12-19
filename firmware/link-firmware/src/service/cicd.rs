use embassy_time::{Duration, Timer};

use super::{Bus, Dispatch, blinken_light};

#[embassy_executor::task]
pub async fn cicd_service(mut bus: Bus) -> ! {
	loop {
		bus.dispatch(blinken_light::Message::On).await;
		Timer::after(Duration::from_millis(60000 / 140 / 2)).await;
		bus.dispatch(blinken_light::Message::Off).await;
		Timer::after(Duration::from_millis(60000 / 140 / 2)).await;
	}
}
