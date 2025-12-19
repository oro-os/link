use embassy_stm32::gpio::OutputOpenDrain;
use embassy_time::Duration;

use crate::channel::{ChannelExt, ReceiveDelay};

pub type Channel = crate::channel::Channel<Message, 2>;

#[derive(defmt::Format)]
#[allow(unused)]
pub enum Message {
	On,
	Idle,
	Off,
}

async fn blink_cycle<T>(
	recv: &mut impl ReceiveDelay<T>,
	leds: &mut [OutputOpenDrain<'static>],
	cycle_delay: Option<Duration>,
) -> Result<!, T> {
	loop {
		for led in leds.iter_mut() {
			for _ in 0..2 {
				led.set_low();
				recv.after_receive(Duration::from_millis(10)).await?;
				led.set_high();
				recv.after_receive(Duration::from_millis(20)).await?;
			}
			recv.after_receive(Duration::from_millis(30)).await?;
		}

		if let Some(delay) = cycle_delay {
			for led in leds.iter_mut() {
				led.set_high();
			}
			recv.after_receive(delay).await?;
		}
	}
}

#[embassy_executor::task]
pub async fn blinken_light(
	mut recv: <Channel as ChannelExt>::Receiver,
	debug_led1: OutputOpenDrain<'static>,
	debug_led2: OutputOpenDrain<'static>,
	debug_led3: OutputOpenDrain<'static>,
) -> ! {
	let mut leds = [debug_led1, debug_led2, debug_led3];

	let mut mode = Message::On;

	loop {
		mode = match mode {
			Message::On => blink_cycle(&mut recv, &mut leds, None).await.unwrap_err(),
			Message::Idle => {
				blink_cycle(&mut recv, &mut leds, Some(Duration::from_secs(10)))
					.await
					.unwrap_err()
			}
			Message::Off => {
				for led in leds.iter_mut() {
					led.set_high();
				}
				recv.receive().await
			}
		}
	}
}
