//! Delta force blinking lights operation supreme, go.

use embassy_stm32::gpio::OutputOpenDrain;
use embassy_time::Duration;

use crate::channel::ReceiveDelay;

pub type Channel = crate::channel::Channel<Cmd, 2>;

#[expect(unused)]
pub enum Cmd {
	On,
	Idle,
	Off,
}

pub struct Config {
	pub debug_led1: OutputOpenDrain<'static>,
	pub debug_led2: OutputOpenDrain<'static>,
	pub debug_led3: OutputOpenDrain<'static>,
}

async fn blink_cycle(
	rx: &Channel,
	leds: &mut [OutputOpenDrain<'static>],
	cycle_delay: Option<Duration>,
) -> Result<!, Cmd> {
	loop {
		for led in leds.iter_mut() {
			for _ in 0..2 {
				led.set_low();
				rx.after_receive(Duration::from_millis(10)).await?;
				led.set_high();
				rx.after_receive(Duration::from_millis(20)).await?;
			}
			rx.after_receive(Duration::from_millis(30)).await?;
		}

		if let Some(delay) = cycle_delay {
			for led in leds.iter_mut() {
				led.set_high();
			}
			rx.after_receive(delay).await?;
		}
	}
}

#[embassy_executor::task]
pub async fn run(rx: &'static Channel, config: Config) -> ! {
	let Config {
		debug_led1,
		debug_led2,
		debug_led3,
	} = config;
	let mut leds = [debug_led1, debug_led2, debug_led3];
	let mut mode = Cmd::On;

	loop {
		mode = match mode {
			Cmd::On => blink_cycle(rx, &mut leds, None).await.unwrap_err(),
			Cmd::Idle => {
				blink_cycle(rx, &mut leds, Some(Duration::from_secs(10)))
					.await
					.unwrap_err()
			}
			Cmd::Off => {
				for led in leds.iter_mut() {
					led.set_high();
				}
				rx.receive().await
			}
		}
	}
}
