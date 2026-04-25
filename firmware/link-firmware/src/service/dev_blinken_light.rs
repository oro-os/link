//! Delta force blinking lights operation supreme, go.

use embassy_futures::select::Either;
use embassy_stm32::gpio::OutputOpenDrain;
use embassy_time::{Duration, Timer};

use crate::channel::ReceiveDelay;

pub const CONFIG_DUTY_PERIOD: u16 = 1000;

pub type Channel = crate::channel::Channel<Cmd, 2>;

#[derive(Clone, Copy)]
pub enum Cmd {
	On,
	Idle,
	Config,
	Off,
	Error,
	Manual { states: [bool; 3] },
}

impl From<Cmd> for heapless::String<16> {
	fn from(v: Cmd) -> Self {
		match v {
			Cmd::On => "all on".try_into().unwrap(),
			Cmd::Idle => "idle".try_into().unwrap(),
			Cmd::Config => "config".try_into().unwrap(),
			Cmd::Off => "all off".try_into().unwrap(),
			Cmd::Error => "error".try_into().unwrap(),
			Cmd::Manual { states } => {
				match (states[0], states[1], states[2]) {
					(false, false, false) => "off, off, off",
					(false, false, true) => "off, off, on",
					(false, true, false) => "off, on, off",
					(false, true, true) => "off, on, on",
					(true, false, false) => "on, off, off",
					(true, false, true) => "on, off, on",
					(true, true, false) => "on, on, off",
					(true, true, true) => "on, on, on",
				}
				.try_into()
				.unwrap()
			}
		}
	}
}

pub struct Config {
	pub debug_led1: OutputOpenDrain<'static>,
	pub debug_led2: OutputOpenDrain<'static>,
	pub debug_led3: OutputOpenDrain<'static>,
}

async fn blink_cycle(
	rx: &Channel,
	mut leds: [&mut OutputOpenDrain<'static>; 3],
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
			for led in &mut leds {
				led.set_high();
			}
			rx.after_receive(delay).await?;
		}
	}
}

async fn config_cycle(rx: &Channel, leds: [&mut OutputOpenDrain<'static>; 3]) -> Result<!, Cmd> {
	let mut i1 = (0..CONFIG_DUTY_PERIOD)
		.chain((0..CONFIG_DUTY_PERIOD).rev())
		.chain((0..1).cycle().take(1000))
		.cycle();
	let mut i2 = (0..CONFIG_DUTY_PERIOD)
		.chain((0..CONFIG_DUTY_PERIOD).rev())
		.chain((0..1).cycle().take(1000))
		.cycle()
		.skip((CONFIG_DUTY_PERIOD / 3) as usize);
	let mut i3 = (0..CONFIG_DUTY_PERIOD)
		.chain((0..CONFIG_DUTY_PERIOD).rev())
		.chain((0..1).cycle().take(1000))
		.cycle()
		.skip((CONFIG_DUTY_PERIOD / 3) as usize * 2);

	let [l1, l2, l3] = leds;

	loop {
		let d1 = i1.next().unwrap_or(0);
		let d2 = i2.next().unwrap_or(0);
		let d3 = i3.next().unwrap_or(0);

		let p1 = async {
			if d1 > 0 {
				l1.set_low();
				Timer::after_micros(u64::from(d1)).await;
			}
			l1.set_high();
			Timer::after_micros(u64::from(CONFIG_DUTY_PERIOD - d1)).await;
		};
		let p2 = async {
			if d2 > 0 {
				l2.set_low();
				Timer::after_micros(u64::from(d2)).await;
			}
			l2.set_high();
			Timer::after_micros(u64::from(CONFIG_DUTY_PERIOD - d2)).await;
		};
		let p3 = async {
			if d3 > 0 {
				l3.set_low();
				Timer::after_micros(u64::from(d3)).await;
			}
			l3.set_high();
			Timer::after_micros(u64::from(CONFIG_DUTY_PERIOD - d3)).await;
		};

		if let Either::First(ev) =
			embassy_futures::select::select(rx.receive(), embassy_futures::join::join3(p1, p2, p3))
				.await
		{
			return Err(ev);
		}
	}
}

async fn error_cycle(rx: &Channel, mut leds: [&mut OutputOpenDrain<'static>; 3]) -> Result<!, Cmd> {
	loop {
		for led in leds.iter_mut() {
			led.set_low();
		}

		rx.after_receive(Duration::from_millis(100)).await?;

		for led in leds.iter_mut() {
			led.set_high();
		}

		rx.after_receive(Duration::from_millis(100)).await?;
	}
}

#[embassy_executor::task]
pub async fn run(rx: &'static Channel, config: Config) -> ! {
	let Config {
		mut debug_led1,
		mut debug_led2,
		mut debug_led3,
	} = config;

	let mut mode = Cmd::On;

	loop {
		crate::vars::STAT_BLINKEN_LIGHT_CMD.set(mode.into());

		mode = match mode {
			Cmd::On => {
				blink_cycle(
					rx,
					[&mut debug_led1, &mut debug_led2, &mut debug_led3],
					None,
				)
				.await
				.unwrap_err()
			}
			Cmd::Config => {
				config_cycle(rx, [&mut debug_led1, &mut debug_led2, &mut debug_led3])
					.await
					.unwrap_err()
			}
			Cmd::Idle => {
				blink_cycle(
					rx,
					[&mut debug_led1, &mut debug_led2, &mut debug_led3],
					Some(Duration::from_secs(10)),
				)
				.await
				.unwrap_err()
			}
			Cmd::Off => {
				debug_led1.set_high();
				debug_led2.set_high();
				debug_led3.set_high();
				rx.receive().await
			}
			Cmd::Manual { states } => {
				if states[0] {
					debug_led1.set_low();
				} else {
					debug_led1.set_high();
				}
				if states[1] {
					debug_led2.set_low();
				} else {
					debug_led2.set_high();
				}
				if states[2] {
					debug_led3.set_low();
				} else {
					debug_led3.set_high();
				}
				rx.receive().await
			}
			Cmd::Error => {
				error_cycle(rx, [&mut debug_led1, &mut debug_led2, &mut debug_led3])
					.await
					.unwrap_err()
			}
		}
	}
}
