use embassy_time::{Duration, Timer};

use super::{Bus, Dispatch, blinken_light};
use crate::service::led_controller::{self};

#[embassy_executor::task]
pub async fn cicd_service(mut bus: Bus) -> ! {
	bus.dispatch(blinken_light::Message::On).await;
	bus.dispatch(led_controller::Message::SelfTest).await;
	Timer::after(Duration::from_secs(4)).await;

	bus.dispatch(led_controller::Message::AllOff).await;
	bus.dispatch(led_controller::Message::SetSdCableIndicator(true))
		.await;
	bus.dispatch(led_controller::Message::SetSdCardIndicator(true))
		.await;
	bus.dispatch(led_controller::Message::SetSdSenseIndicator(true))
		.await;

	let mut time: f32 = 0.0;
	let mut idle = false;
	let mut idle_after = 100;
	loop {
		// RGB cycle
		use micromath::F32Ext;
		let r = ((time * 0.3).sin() * 0.5 + 0.5) * 255.0;
		let g = ((time * 0.3 + 2.0).sin() * 0.5 + 0.5) * 255.0;
		let b = ((time * 0.3 + 4.0).sin() * 0.5 + 0.5) * 255.0;
		let rgb = (r as u8, g as u8, b as u8).into();
		bus.dispatch(led_controller::Message::SetBacklight(rgb))
			.await;
		bus.dispatch(led_controller::Message::SetSystemIndicator(rgb))
			.await;
		bus.dispatch(led_controller::Message::SetRemoteIndicator(rgb))
			.await;
		bus.dispatch(led_controller::Message::SetJobIndicator(rgb))
			.await;
		time += 0.05;

		idle_after -= 1;
		if idle_after == 0 {
			idle = !idle;
			idle_after = 100;
			bus.dispatch(led_controller::Message::SetIdle(idle)).await;
		}

		Timer::after(Duration::from_millis(1000 / 60)).await;
	}
}
