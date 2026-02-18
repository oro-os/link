use embassy_time::{Duration, Timer};

use super::{dev_blinken_light, dev_leds, dev_oled};

#[embassy_executor::task]
pub async fn run(bus: &'static super::Bus) -> ! {
	bus.dev_oled
		.send(dev_oled::Cmd::SetState(dev_oled::State::On))
		.await;

	bus.dev_blinken_light.send(dev_blinken_light::Cmd::On).await;
	bus.dev_leds.send(dev_leds::Cmd::SelfTest).await;
	Timer::after(Duration::from_secs(4)).await;

	bus.dev_leds.send(dev_leds::Cmd::AllOff).await;
	bus.dev_leds
		.send(dev_leds::Cmd::SetSdCableIndicator(true))
		.await;
	bus.dev_leds
		.send(dev_leds::Cmd::SetSdCardIndicator(true))
		.await;
	bus.dev_leds
		.send(dev_leds::Cmd::SetSdSenseIndicator(true))
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
		bus.dev_leds.send(dev_leds::Cmd::SetBacklight(rgb)).await;
		bus.dev_leds
			.send(dev_leds::Cmd::SetSystemIndicator(rgb))
			.await;
		bus.dev_leds
			.send(dev_leds::Cmd::SetRemoteIndicator(rgb))
			.await;
		bus.dev_leds.send(dev_leds::Cmd::SetJobIndicator(rgb)).await;
		time += 0.05;

		idle_after -= 1;
		if idle_after == 0 {
			idle = !idle;
			idle_after = 100;
			bus.dev_leds.send(dev_leds::Cmd::SetIdle(idle)).await;
		}

		Timer::after(Duration::from_millis(1000 / 60)).await;
	}
}
