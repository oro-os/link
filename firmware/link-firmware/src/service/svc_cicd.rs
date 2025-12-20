use embassy_time::{Duration, Timer};

use super::{Bus, Dispatch, dev_blinken_light, dev_leds, dev_oled};

#[embassy_executor::task]
pub async fn run(mut bus: Bus) -> ! {
	bus.dispatch(dev_oled::Message::SetState(dev_oled::State::On))
		.await;

	bus.dispatch(dev_blinken_light::Message::On).await;
	bus.dispatch(dev_leds::Message::SelfTest).await;
	Timer::after(Duration::from_secs(4)).await;

	bus.dispatch(dev_leds::Message::AllOff).await;
	bus.dispatch(dev_leds::Message::SetSdCableIndicator(true))
		.await;
	bus.dispatch(dev_leds::Message::SetSdCardIndicator(true))
		.await;
	bus.dispatch(dev_leds::Message::SetSdSenseIndicator(true))
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
		bus.dispatch(dev_leds::Message::SetBacklight(rgb)).await;
		bus.dispatch(dev_leds::Message::SetSystemIndicator(rgb))
			.await;
		bus.dispatch(dev_leds::Message::SetRemoteIndicator(rgb))
			.await;
		bus.dispatch(dev_leds::Message::SetJobIndicator(rgb)).await;
		time += 0.05;

		idle_after -= 1;
		if idle_after == 0 {
			idle = !idle;
			idle_after = 100;
			bus.dispatch(dev_leds::Message::SetIdle(idle)).await;
		}

		Timer::after(Duration::from_millis(1000 / 60)).await;
	}
}
