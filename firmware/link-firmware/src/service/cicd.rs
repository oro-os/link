use embassy_time::{Duration, Timer};

use super::{Bus, Dispatch, blinken_light};
use crate::service::led_controller::{self};

#[embassy_executor::task]
pub async fn cicd_service(mut bus: Bus) -> ! {
	bus.dispatch(blinken_light::Message::On).await;
	bus.dispatch(led_controller::Message::SelfTest).await;
	Timer::after(Duration::from_secs(4)).await;

	let mut time: f32 = 0.0;
	loop {
		// RGB cycle
		use micromath::F32Ext;
		let r = ((time * 0.3).sin() * 0.5 + 0.5) * 255.0;
		let g = ((time * 0.3 + 2.0).sin() * 0.5 + 0.5) * 255.0;
		let b = ((time * 0.3 + 4.0).sin() * 0.5 + 0.5) * 255.0;
		bus.dispatch(led_controller::Message::SetBacklight(
			(r as u8, g as u8, b as u8).into(),
		))
		.await;
		time += 0.05;
		Timer::after(Duration::from_millis(1000 / 60)).await;
	}
}
