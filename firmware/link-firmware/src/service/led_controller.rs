use defmt::{error, info};
use embassy_stm32::gpio::Output;
use embassy_stm32::{i2c::I2c, mode::Async};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Channel, mutex::Mutex};
use embassy_time::{Duration, Timer};

const ADDR: u8 = 0b01111000 >> 1;

#[embassy_executor::task]
pub async fn led_controller(
	i2c: &'static Mutex<NoopRawMutex, I2c<'static, Async>>,
	mut enable_chip: Output<'static>,
) -> ! {
	enable_chip.set_high();
	Timer::after(Duration::from_millis(100)).await;

	let mut led = IS31FL3236A::new(i2c);
	led.reset().await;
	Timer::after(Duration::from_millis(1)).await;
	led.set_is_shutdown(false).await;
	Timer::after(Duration::from_millis(1)).await;
	led.set_frequency(OutputFrequency::Khz3).await;
	led.present_state().await;
	led.present_pwm().await;

	loop {
		Timer::after(Duration::from_millis(5000)).await;
	}
}

struct IS31FL3236A {
	i2c: &'static Mutex<NoopRawMutex, I2c<'static, Async>>,
	pwm_state: [u8; 38], // 36 + 1 for cursor + 1 for update
	ch_state: [u8; 37],  // 36 + 1 for cursor
}

#[expect(dead_code)]
impl IS31FL3236A {
	fn new(i2c: &'static Mutex<NoopRawMutex, I2c<'static, Async>>) -> Self {
		let mut this = Self {
			i2c,
			pwm_state: [0; 38],
			ch_state: [0; 37],
		};

		this.pwm_state[0] = 0x01;
		this.ch_state[0] = 0x26;
		this
	}

	async fn write(&self, data: &[u8]) {
		let mut i2c = self.i2c.lock().await;
		if let Err(err) = i2c.blocking_write(ADDR, data) {
			error!("failed to write to LED controller chip: {:?}", err);
		}
	}

	fn set_pwm(&mut self, channel: usize, value: u8) {
		debug_assert!(channel > 0 && channel < 37);
		self.pwm_state[channel] = value;
	}

	fn set_ch_state(&mut self, channel: usize, value: ChannelState) {
		debug_assert!(channel > 0 && channel < 37);
		self.ch_state[channel] = value.into();
	}

	async fn set_is_shutdown(&self, is_shutdown: bool) {
		self.write(&[0x00, if is_shutdown { 0x00 } else { 0x01 }])
			.await;
	}

	async fn reset(&self) {
		self.write(&[0x4F, 0x00]).await;
		Timer::after(Duration::from_millis(1)).await;
	}

	async fn present_pwm(&self) {
		self.write(&self.pwm_state).await;
	}

	async fn present_state(&self) {
		self.write(&self.ch_state).await;
		self.write(&[0x25, 0x00]).await;
	}

	async fn enable_all_channels(&self) {
		self.write(&[0x4A, 0x00]).await;
	}

	async fn disable_all_channels(&self) {
		self.write(&[0x4A, 0x01]).await;
	}

	async fn set_frequency(&self, frequency: OutputFrequency) {
		self.write(&[0x4B, frequency as u8]).await;
	}
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct ChannelState(u8);

#[expect(dead_code)]
impl ChannelState {
	pub const fn new() -> Self {
		Self(0)
	}

	pub const fn with_on(self) -> Self {
		Self(self.0 | 0x01)
	}

	pub const fn with_off(self) -> Self {
		Self(self.0 & !0x01)
	}

	pub const fn with_max_current(self, max_current: MaxCurrent) -> Self {
		Self((self.0 & !6) | ((max_current as u8) << 1))
	}
}

impl From<u8> for ChannelState {
	#[inline]
	fn from(value: u8) -> Self {
		Self(value)
	}
}

impl From<ChannelState> for u8 {
	#[inline]
	fn from(value: ChannelState) -> Self {
		value.0
	}
}

#[derive(Clone, Copy)]
#[repr(u8)]
#[expect(dead_code)]
enum MaxCurrent {
	Imax = 0b00,
	ImaxDiv2 = 0b01,
	ImaxDiv3 = 0b10,
	ImaxDiv4 = 0b11,
}

#[derive(Clone, Copy)]
#[repr(u8)]
#[expect(dead_code)]
enum OutputFrequency {
	Khz3 = 0b0,
	Khz22 = 0b1,
}
