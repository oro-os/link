use core::sync::atomic::AtomicU8;

use embassy_executor::Spawner;
use embassy_stm32::{
	gpio::Output,
	i2c::{I2c, mode::Master},
	mode::Blocking,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};
use static_cell::StaticCell;

use crate::{
	atomic::Relaxed,
	channel::{Channel as RawChannel, Receiver, Sender},
	color::Rgb,
	service::svc_mqtt_stats::BoolStat,
};

const ADDR: u8 = 0b01111000 >> 1;
const MAX_CURRENT: MaxCurrent = MaxCurrent::ImaxDiv4;
const SHIFT_CHANNEL: usize = 0; // Channel 0 is a "special" channel that sets the brightness shift for all channels.
const IDLE_SHIFT: u8 = 4; // Divide by 16.
const IDLE_MIN: u8 = 2; // Minimum brightness in idle mode (for non-zero brightness).

pub type Channel = crate::channel::Channel<Cmd, 4>;

type RgbReceiver = Receiver<Rgb, 2>;
type LightSender = Sender<(usize, u8), 16>;
type OnOffReceiver = Receiver<bool, 2>;
type GreyReceiver = Receiver<u8, 2>;

pub static STAT_CHIP_ENABLED: BoolStat = BoolStat::new("status/leds_enabled");

pub static DBG_LIGHT_VALUES: [AtomicU8; 36] = [const { AtomicU8::new(0) }; 36];

#[derive(defmt::Format)]
#[allow(unused)]
pub enum Cmd {
	/// Turns off all lights
	AllOff,
	/// Performs a self-test sequence; no
	/// commands will be processed until
	/// the self-test is complete.
	SelfTest,
	/// Sets the controller to idle mode,
	/// dimming all lights.
	SetIdle(bool),
	/// Sets the backlight color.
	SetBacklight(Rgb),
	/// Sets the system indicator color.
	///
	/// Specifying `None` turns off the system indicator.
	SetSystemIndicator(Rgb),
	/// Sets the remote indicator color.
	///
	/// Specifying `None` turns off the remote indicator.
	SetRemoteIndicator(Rgb),
	/// Sets the job indicator color.
	///
	/// Specifying `None` turns off the job indicator.
	SetJobIndicator(Rgb),
	/// Sets the SD ribbon cable (SUT) sense indicator on or off.
	SetSdCableIndicator(bool),
	/// Sets the SD card activity indicator on or off.
	SetSdCardIndicator(bool),
	/// Sets the SD card sense indicator on or off.
	SetSdSenseIndicator(bool),
	/// Sets the manual state
	SetManualState { state: [u8; 36] },
}

pub struct Config {
	pub spawner:     Spawner,
	pub i2c:         &'static Mutex<NoopRawMutex, I2c<'static, Blocking, Master>>,
	pub enable_chip: Output<'static>,
}

#[embassy_executor::task]
pub async fn run(recv: &'static Channel, config: Config) -> ! {
	let Config {
		spawner,
		i2c,
		mut enable_chip,
	} = config;

	STAT_CHIP_ENABLED.set(false);
	enable_chip.set_high();
	Timer::after(Duration::from_millis(100)).await;
	STAT_CHIP_ENABLED.set(true);

	let mut led = IS31FL3236A::new(i2c);
	led.reset().await;
	Timer::after(Duration::from_millis(1)).await;
	led.set_is_shutdown(false).await;
	led.enable_all_channels().await;
	Timer::after(Duration::from_millis(1)).await;
	led.set_frequency(OutputFrequency::Khz3).await;

	for channel in 1..=36 {
		led.set_ch_state(
			channel,
			ChannelState::new().with_max_current(MAX_CURRENT).with_on(),
		);
		led.set_pwm(channel, 0);
	}
	led.present_state().await;
	led.present_pwm().await;

	static LIGHT_CHANNEL: static_cell::StaticCell<RawChannel<(usize, u8), 16>> =
		static_cell::StaticCell::new();
	let light_ch = LIGHT_CHANNEL.init(RawChannel::new());

	macro_rules! setup_lights {
		($(let $name:ident = $ty:tt($($tt:tt)+));+ $(;)?) => {
			$(
				let $name = setup_lights!(@task $ty($($tt)+));
			)+
		};
		(@task Rgb($ch_r:expr, $ch_g:expr, $ch_b:expr)) => {
			{
				static CHANNEL: StaticCell<RawChannel<Rgb, 2>> = StaticCell::new();
				let ch = CHANNEL.init(RawChannel::new());
				let rx = ch.receiver();
				spawner.spawn(rgb_light_task(rx, light_ch.sender(), $ch_r, $ch_g, $ch_b).unwrap());
				ch.sender()
			}
		};
		(@task OnOff($ch_bool:expr)) => {
			{
				static CHANNEL: StaticCell<RawChannel<bool, 2>> = StaticCell::new();
				let ch = CHANNEL.init(RawChannel::new());
				let rx = ch.receiver();
				spawner.spawn(on_off_light_task(rx, light_ch.sender(), $ch_bool).unwrap());
				ch.sender()
			}
		};
		(@task MultiRgb($([$r:expr, $g:expr, $b:expr]),* $(,)?)) => {
			{
				static CH_R: &'static [usize] = &[$($r),*];
				static CH_G: &'static [usize] = &[$($g),*];
				static CH_B: &'static [usize] = &[$($b),*];
				static CHANNEL: StaticCell<RawChannel<Rgb, 2>> = StaticCell::new();
				let ch = CHANNEL.init(RawChannel::new());
				let rx = ch.receiver();
				spawner.spawn(multi_rgb_light_task(rx, light_ch.sender(), CH_R, CH_G, CH_B).unwrap());
				ch.sender()
			}
		};
		(@task MultiGrey($($ch_grey:expr),* $(,)?)) => {
			{
				static CH_GREY: &'static [usize] = &[$($ch_grey),*];
				static CHANNEL: StaticCell<RawChannel<u8, 2>> = StaticCell::new();
				let ch = CHANNEL.init(RawChannel::new());
				let rx = ch.receiver();
				spawner.spawn(multi_grey_light_task(rx, light_ch.sender(), CH_GREY).unwrap());
				ch.sender()
			}
		};
	}

	setup_lights! {
		// SD card lights
		// These are subject to change - see
		// https://github.com/oro-os/link/issues/109
		let sd_cable_sense_led = OnOff(36);
		let sd_card_activity_led = OnOff(2);
		let sd_card_sense_led = OnOff(1);

		// Status lights
		let remote_link_led = Rgb(31, 30, 32);
		let job_indicator_led = Rgb(28, 29, 27);
		let system_indicator_led = Rgb(34, 33, 35);

		// Backlight
		let backlight_rgb_led = MultiRgb(
			[19, 18, 20],
			[10, 9, 11],
			[5, 6, 4],
			[15, 16, 17],
			[13, 12, 14],
			[25, 26, 24],
			[21, 22, 23],
		);

		let backlight_white_led = MultiGrey(7, 8, 3);
	}

	spawner.spawn(presenter_task(light_ch.receiver(), led).unwrap());

	loop {
		let msg = recv.receive().await;
		defmt::trace!("got command {:?}", msg);
		match msg {
			Cmd::AllOff => {
				for channel in 1..=36 {
					light_ch.send((channel, 0)).await;
				}
			}
			Cmd::SelfTest => {
				for _ in 0..3 {
					for channel in 1..=36 {
						light_ch.send((channel, 255)).await;
					}
					Timer::after(Duration::from_millis(250)).await;
					for channel in 1..=36 {
						light_ch.send((channel, 0)).await;
					}
					Timer::after(Duration::from_millis(250)).await;
				}

				for channel in 1..=36 {
					light_ch.send((channel, 255)).await;
					Timer::after(Duration::from_millis(50)).await;
					light_ch.send((channel, 0)).await;
				}
			}
			Cmd::SetIdle(is_idle) => {
				light_ch
					.send((SHIFT_CHANNEL, if is_idle { IDLE_SHIFT } else { 0 }))
					.await;
			}
			Cmd::SetJobIndicator(rgb) => {
				job_indicator_led.send(rgb).await;
			}
			Cmd::SetRemoteIndicator(rgb) => {
				remote_link_led.send(rgb).await;
			}
			Cmd::SetSystemIndicator(rgb) => {
				system_indicator_led.send(rgb).await;
			}
			Cmd::SetBacklight(rgb) => {
				backlight_rgb_led.send(rgb.without_white_component()).await;
				backlight_white_led.send(rgb.white_component()).await;
			}
			Cmd::SetSdCableIndicator(on) => {
				sd_cable_sense_led.send(on).await;
			}
			Cmd::SetSdCardIndicator(on) => {
				sd_card_activity_led.send(on).await;
			}
			Cmd::SetSdSenseIndicator(on) => {
				sd_card_sense_led.send(on).await;
			}
			Cmd::SetManualState { state } => {
				for (i, b) in state.into_iter().enumerate() {
					light_ch.send((i + 1, b)).await;
				}
			}
		}
		defmt::trace!("command processed");
	}
}

#[embassy_executor::task]
async fn presenter_task(light_ch: Receiver<(usize, u8), 16>, mut led: IS31FL3236A) -> ! {
	trait SetChannel {
		fn update_channel(&mut self, ch: usize, brightness: u8);
	}

	impl SetChannel for IS31FL3236A {
		fn update_channel(&mut self, ch: usize, brightness: u8) {
			if ch == SHIFT_CHANNEL {
				self.set_shift(brightness);
			} else if brightness > 0 {
				self.set_pwm(ch, brightness);
				self.set_ch_state(
					ch,
					ChannelState::new().with_max_current(MAX_CURRENT).with_on(),
				);
			} else {
				self.set_pwm(ch, 0);
				self.set_ch_state(
					ch,
					ChannelState::new().with_max_current(MAX_CURRENT).with_off(),
				);
			}
		}
	}

	loop {
		let (ch, brightness) = light_ch.receive().await;

		led.update_channel(ch, brightness);

		// Update all others, if there are more pending events, instead
		// of performing an I2C transaction for each small update.
		while let Ok((ch, brightness)) = light_ch.try_receive() {
			led.update_channel(ch, brightness);
		}

		led.present_pwm().await;
	}
}

#[embassy_executor::task]
async fn multi_rgb_light_task(
	rx: RgbReceiver,
	tx: LightSender,
	ch_r: &'static [usize],
	ch_g: &'static [usize],
	ch_b: &'static [usize],
) -> ! {
	loop {
		let color = rx.receive().await;

		let (r, g, b) = color.into();
		for &ch in ch_r {
			tx.send((ch, r)).await;
		}
		for &ch in ch_g {
			tx.send((ch, g)).await;
		}
		for &ch in ch_b {
			tx.send((ch, b)).await;
		}
	}
}

#[embassy_executor::task(pool_size = 3)]
async fn rgb_light_task(
	rx: RgbReceiver,
	tx: LightSender,
	ch_r: usize,
	ch_g: usize,
	ch_b: usize,
) -> ! {
	loop {
		let color = rx.receive().await;
		let (r, g, b) = color.into();
		tx.send((ch_r, r)).await;
		tx.send((ch_g, g)).await;
		tx.send((ch_b, b)).await;
	}
}

#[embassy_executor::task]
async fn multi_grey_light_task(rx: GreyReceiver, tx: LightSender, ch_grey: &'static [usize]) -> ! {
	loop {
		let color = rx.receive().await;
		for &ch in ch_grey {
			tx.send((ch, color)).await;
		}
	}
}

#[embassy_executor::task(pool_size = 3)]
async fn on_off_light_task(rx: OnOffReceiver, tx: Sender<(usize, u8)>, ch: usize) -> ! {
	let mut state = false;
	loop {
		let new_state = rx.receive().await;
		if new_state != state {
			state = new_state;
			let brightness = if state { 255 } else { 0 };
			tx.send((ch, brightness)).await;
		}
	}
}

struct IS31FL3236A {
	i2c:          &'static Mutex<NoopRawMutex, I2c<'static, Blocking, Master>>,
	pwm_state:    [u8; 38], // 36 + 1 for cursor + 1 for update
	ch_state:     [u8; 37], // 36 + 1 for cursor
	global_shift: u8,
}

#[expect(dead_code)]
impl IS31FL3236A {
	fn new(i2c: &'static Mutex<NoopRawMutex, I2c<'static, Blocking, Master>>) -> Self {
		let mut this = Self {
			i2c,
			pwm_state: [0; 38],
			ch_state: [0; 37],
			global_shift: 0,
		};

		this.pwm_state[0] = 0x01;
		this.ch_state[0] = 0x26;
		this
	}

	async fn write(&self, data: &[u8]) {
		let mut i2c = self.i2c.lock().await;
		if let Err(err) = i2c.blocking_write(ADDR, data) {
			defmt::error!("failed to write to LED controller chip: {:?}", err);
		}
	}

	fn set_shift(&mut self, shift: u8) {
		self.global_shift = shift;
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
		if self.global_shift > 0 {
			let mut shifted_pwm = [0u8; 38];
			shifted_pwm.copy_from_slice(&self.pwm_state);

			for (i, pwm) in shifted_pwm[1..=36].iter_mut().enumerate() {
				if *pwm > 0 {
					*pwm = (*pwm >> self.global_shift).max(IDLE_MIN);
				}
				DBG_LIGHT_VALUES[i].set(*pwm);
			}

			self.write(&shifted_pwm).await;
		} else {
			self.write(&self.pwm_state).await;
			for (i, pwm) in self.pwm_state.iter().skip(1).take(36).enumerate() {
				DBG_LIGHT_VALUES[i].set(*pwm);
			}
		}
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

impl ChannelState {
	const fn new() -> Self {
		Self(0)
	}

	const fn with_on(self) -> Self {
		Self(self.0 | 0x01)
	}

	const fn with_off(self) -> Self {
		Self(self.0 & !0x01)
	}

	const fn with_max_current(self, max_current: MaxCurrent) -> Self {
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
	Imax     = 0b00,
	ImaxDiv2 = 0b01,
	ImaxDiv3 = 0b10,
	ImaxDiv4 = 0b11,
}

#[derive(Clone, Copy)]
#[repr(u8)]
#[expect(dead_code)]
enum OutputFrequency {
	Khz3  = 0b0,
	Khz22 = 0b1,
}
