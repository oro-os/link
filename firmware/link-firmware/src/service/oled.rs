use embassy_executor::Spawner;
use embassy_stm32::{
	gpio::{Output, OutputOpenDrain},
	mode::Async,
	spi::{Error, Spi},
};
use embassy_time::{Duration, Timer};
use embedded_graphics::{
	framebuffer::{Framebuffer, buffer_size},
	pixelcolor::{Gray4, PixelColor, raw::LittleEndian},
	prelude::*,
};
use embedded_graphics_core::{
	draw_target::DrawTarget,
	geometry::{OriginDimensions, Size},
	primitives::Rectangle,
};
use embedded_hal_async::spi::SpiBus;

use crate::channel::{Channel as RawChannel, ChannelExt, ReceiveDelay, Receiver, Sender};

// const IDLE_WAIT_DURATION: Duration = Duration::from_secs(60 * 5); // 5 minutes
const IDLE_WAIT_DURATION: Duration = Duration::from_secs(10);
const IDLE_COOL_OFF_STEP_DURATION: Duration = Duration::from_millis(100);
const IDLE_COOL_OFF_STEP: u8 = 1; // Decrease brightness by 5 each step
const IDLE_MIN_BRIGHTNESS: u8 = 80; // Minimum brightness before vreg shutoff
const IDLE_VREG_OFF_DELAY: Duration = Duration::from_secs(10); // Time after turning off display to turn off VREG

const BRIGHTNESS_CURVE: [u8; 64] = [
	0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 8, 8, 9, 10, 11, 12, 12, 13,
	14, 15, 16, 17, 18, 19, 21, 22, 23, 24, 25, 27, 28, 29, 31, 32, 34, 35, 37, 38, 40, 41, 43, 45,
	46, 48, 50, 52, 53, 55, 57, 59, 61, 63,
];

const ORO_LOGO_COLORS: &[Gray4] = &[
	Gray4::new(0x0),
	Gray4::new(0x5),
	Gray4::new(0xA),
	Gray4::new(0xF),
];

type OroLogo = oro_logo_rle::OroLogo<oro_logo_rle::OroLogo64x64>;

pub type Channel = RawChannel<Message, 4>;

#[derive(defmt::Format)]
#[allow(unused)]
pub enum State {
	/// OLED should remain fully on
	On,
	/// OLED will slowly turn off after idle period
	Idle,
	/// OLED should immediately turn off
	Off,
}

#[derive(defmt::Format, Clone, Copy, PartialEq, Eq)]
#[allow(unused)]
pub enum Scene {
	Logo,
}

#[derive(defmt::Format)]
#[allow(unused)]
pub enum Message {
	SetState(State),
	SetScene(Scene),
}

#[derive(defmt::Format)]
#[allow(unused)]
enum DriverMessage {
	SetPower(bool),
	Render,
	SetBrightness(u8),
	Message(Message),
	ForceSetScene(Scene),
}

struct LogoScene {
	logo_iter: OroLogo,
}

impl LogoScene {
	fn new() -> Self {
		Self {
			logo_iter: OroLogo::new(),
		}
	}

	fn render(&mut self, fb: &mut FrameBuf) -> Duration {
		use oro_logo_rle::{Command, OroLogoData};

		let mut off = 0;

		loop {
			match self.logo_iter.next() {
				None => panic!("Oro logo exhausted commands (shouldn't happen)"),

				Some(Command::End) => break,

				Some(Command::Draw(count, lightness)) => {
					let color = ORO_LOGO_COLORS[usize::from(lightness)];

					for i in 0..count {
						let x = ((off + usize::from(i)) % OroLogo::WIDTH)
							+ ((256 / 2) - (OroLogo::WIDTH / 2));
						let y = (off + usize::from(i)) / OroLogo::WIDTH;
						fb.set_pixel(Point::new(x as i32, y as i32), color);
					}

					off += usize::from(count);
				}

				Some(Command::Skip(count)) => {
					off += usize::from(count);
				}
			}
		}

		Duration::from_millis(1000 / OroLogo::FPS as u64)
	}
}

enum SceneInstance {
	Logo(LogoScene),
}

#[embassy_executor::task]
pub async fn oled_service(
	spawner: Spawner,
	recv: <Channel as ChannelExt>::Receiver,
	spi: Spi<'static, Async>,
	cs: OutputOpenDrain<'static>,
	dc: Output<'static>,
	mut rst: Output<'static>,
	mut vreg_en: Output<'static>,
) -> ! {
	static DRIVER_CHANNEL: static_cell::StaticCell<RawChannel<DriverMessage, 4>> =
		static_cell::StaticCell::new();
	let driver_channel = DRIVER_CHANNEL.init(RawChannel::new());
	spawner
		.spawn(oled_driver_task(recv, driver_channel.sender()))
		.unwrap();

	static POWER_STATE_CHANNEL: static_cell::StaticCell<RawChannel<State, 2>> =
		static_cell::StaticCell::new();
	let power_state_channel = POWER_STATE_CHANNEL.init(RawChannel::new());
	spawner
		.spawn(oled_power_state_task(
			power_state_channel.receiver(),
			driver_channel.sender(),
		))
		.unwrap();

	static FRAME_AFTER_CHANNEL: static_cell::StaticCell<RawChannel<Duration, 2>> =
		static_cell::StaticCell::new();
	let frame_after_channel = FRAME_AFTER_CHANNEL.init(RawChannel::new());
	spawner
		.spawn(oled_frame_timing_task(
			frame_after_channel.receiver(),
			driver_channel.sender(),
		))
		.unwrap();

	let mut oled = SSD1362::new(spi, dc, cs);

	let mut vbus_state = false;
	let mut current_scene_tag: Scene = Scene::Logo;
	let mut current_scene: SceneInstance = SceneInstance::Logo(LogoScene::new());

	loop {
		match driver_channel.receive().await {
			DriverMessage::SetPower(enabled) => {
				match (vbus_state, enabled) {
					(false, true) => {
						oled.set_comms_enabled(true);

						defmt::debug!("enabling OLED VBUS");
						vreg_en.set_high();
						Timer::after(Duration::from_millis(10)).await;

						defmt::debug!("resetting OLED after VBUS enable");
						rst.set_low();
						Timer::after(Duration::from_millis(5)).await;
						rst.set_high();
						Timer::after(Duration::from_millis(5)).await;

						defmt::debug!("re-initializing OLED after VBUS enable");
						oled.init(true).await.expect("OLED re-init failed");

						defmt::debug!("repainting OLED after VBUS enable");
						oled.repaint().await.expect("OLED repaint failed");

						driver_channel
							.send(DriverMessage::ForceSetScene(current_scene_tag))
							.await;
					}
					(true, false) => {
						oled.set_comms_enabled(false);

						defmt::debug!("disabling OLED VBUS");
						vreg_en.set_low();
						Timer::after(Duration::from_millis(10)).await;
					}
					_ => {}
				}

				vbus_state = enabled;
			}
			DriverMessage::Message(Message::SetState(state)) => {
				defmt::debug!("OLED state change requested: {:?}", state);
				power_state_channel.send(state).await;
			}
			DriverMessage::SetBrightness(b) => {
				oled.set_contrast(b).await.unwrap();
			}
			DriverMessage::Message(Message::SetScene(scene)) => {
				if scene != current_scene_tag {
					driver_channel
						.send(DriverMessage::ForceSetScene(scene))
						.await;
				}
			}
			DriverMessage::ForceSetScene(scene) => {
				defmt::debug!("OLED scene change requested: {:?}", scene);
				current_scene_tag = scene;

				oled.clear(Gray4::BLACK).unwrap();

				match scene {
					Scene::Logo => {
						let mut logo_scene = LogoScene::new();
						let frame_after = logo_scene.render(&mut oled.framebuf);
						frame_after_channel.send(frame_after).await;
						oled.repaint().await.unwrap();
					}
				}
			}
			DriverMessage::Render => {
				let frame_after = match &mut current_scene {
					SceneInstance::Logo(scene) => scene.render(&mut oled.framebuf),
				};
				frame_after_channel.send(frame_after).await;
				oled.repaint().await.unwrap();
			}
		}
	}
}

#[embassy_executor::task]
async fn oled_driver_task(rx: Receiver<Message, 4>, tx: Sender<DriverMessage, 4>) -> ! {
	// Just convert all incoming messages to DriverMessages and forward them
	loop {
		let msg = rx.receive().await;
		let driver_msg = DriverMessage::Message(msg);
		tx.send(driver_msg).await;
	}
}

async fn perform_idle_cooloff(
	rx: &mut Receiver<State, 2>,
	tx: &mut Sender<DriverMessage, 4>,
) -> Result<!, State> {
	// Transition to On if we aren't
	defmt::debug!("performing OLED idle cool-off; turning on display fully, first");
	perform_turnon_once(tx).await?;

	// Wait for a few minutes of idle before turning off the display
	defmt::debug!(
		"OLED will begin cooldown after idle period: {:?}",
		IDLE_WAIT_DURATION
	);
	rx.after_receive(IDLE_WAIT_DURATION).await?;

	// Gradually decrease brightness
	defmt::debug!("OLED idle cooldown starting");
	let mut brightness: u8 = 255;
	loop {
		if brightness <= IDLE_MIN_BRIGHTNESS {
			break;
		}

		brightness = brightness
			.saturating_sub(IDLE_COOL_OFF_STEP)
			.max(IDLE_MIN_BRIGHTNESS);
		defmt::debug!(
			"OLED idle cooldown step; setting brightness to {} and waiting for {:?}",
			brightness,
			IDLE_COOL_OFF_STEP_DURATION
		);
		tx.send(DriverMessage::SetBrightness(brightness)).await;
		rx.after_receive(IDLE_COOL_OFF_STEP_DURATION).await?;
	}

	// If we've reached here, it means there's no minimum brightness
	// and we can turn off the regulator
	defmt::debug!(
		"OLED idle cooldown has reached minimum brightness; turning off power after {:?}",
		IDLE_VREG_OFF_DELAY
	);
	rx.after_receive(IDLE_VREG_OFF_DELAY).await?;

	defmt::debug!("OLED idle cooldown complete; shutting of OLED");
	tx.send(DriverMessage::SetPower(false)).await;

	// Wait for next state change
	Err(rx.receive().await)
}

async fn perform_shutoff(
	rx: &mut Receiver<State, 2>,
	tx: &mut Sender<DriverMessage, 4>,
) -> Result<!, State> {
	// Immediately turn off display
	tx.send(DriverMessage::SetBrightness(0)).await;

	// Wait a bit before turning off power regulator
	rx.after_receive(IDLE_VREG_OFF_DELAY).await?;
	tx.send(DriverMessage::SetPower(false)).await;

	// Wait for next state change
	Err(rx.receive().await)
}

async fn perform_turnon_once(tx: &mut Sender<DriverMessage, 4>) -> Result<(), State> {
	tx.send(DriverMessage::SetPower(true)).await;
	tx.send(DriverMessage::SetBrightness(255)).await;
	Ok(())
}

async fn perform_turnon(
	rx: &mut Receiver<State, 2>,
	tx: &mut Sender<DriverMessage, 4>,
) -> Result<!, State> {
	// Perform turn on and then halt, waiting for next state change
	perform_turnon_once(tx).await?;
	Err(rx.receive().await)
}

#[embassy_executor::task]
async fn oled_power_state_task(mut rx: Receiver<State, 2>, mut tx: Sender<DriverMessage, 4>) -> ! {
	let mut current_state = State::Off;

	loop {
		current_state = match current_state {
			State::On => perform_turnon(&mut rx, &mut tx).await.unwrap_err(),
			State::Idle => perform_idle_cooloff(&mut rx, &mut tx).await.unwrap_err(),
			State::Off => perform_shutoff(&mut rx, &mut tx).await.unwrap_err(),
		};
	}
}

#[embassy_executor::task]
async fn oled_frame_timing_task(rx: Receiver<Duration, 2>, tx: Sender<DriverMessage, 4>) -> ! {
	loop {
		let wait_duration = rx.receive().await;
		Timer::after(wait_duration).await;
		tx.send(DriverMessage::Render).await;
	}
}

type FrameBuf = Framebuffer<
	Gray4,
	<Gray4 as PixelColor>::Raw,
	LittleEndian,
	256,
	64,
	{ buffer_size::<Gray4>(256, 64) },
>;

struct SSD1362 {
	spi: Spi<'static, Async>,
	dc: Output<'static>,
	cs: OutputOpenDrain<'static>,
	framebuf: FrameBuf,
	comms_enabled: bool,
}

#[expect(dead_code)]
impl SSD1362 {
	fn new(spi: Spi<'static, Async>, dc: Output<'static>, cs: OutputOpenDrain<'static>) -> Self {
		Self {
			spi,
			dc,
			cs,
			framebuf: Framebuffer::new(),
			comms_enabled: false,
		}
	}

	fn set_comms_enabled(&mut self, enabled: bool) {
		self.comms_enabled = enabled;
	}

	async fn flush(&mut self) -> Result<(), Error> {
		if !self.comms_enabled {
			return Ok(());
		}

		<Spi<'static, Async> as SpiBus<u8>>::flush(&mut self.spi).await?;
		Ok(())
	}

	async fn send_cmd(&mut self, cmd: u8) -> Result<(), Error> {
		if !self.comms_enabled {
			return Ok(());
		}

		self.dc.set_low(); // cmd
		self.cs.set_low();
		let r = self.spi.write(&[cmd]).await;
		// Always flush even if write fails
		let fr = self.flush().await;
		self.cs.set_high();
		r?;
		fr?;
		Ok(())
	}

	async fn send_data(&mut self, data: &[u8]) -> Result<(), Error> {
		if !self.comms_enabled {
			return Ok(());
		}

		self.dc.set_high(); // data
		self.cs.set_low();
		let r = self.spi.write(data).await;
		// Always flush even if write fails
		let fr = self.flush().await;
		self.cs.set_high();
		r?;
		fr?;
		Ok(())
	}

	async fn init(&mut self, flip: bool) -> Result<(), Error> {
		self.send_cmd(0xFD).await?; // Set command lock
		self.send_cmd(0x12).await?; // Unlock

		self.send_cmd(0xAE).await?; // Display off

		self.send_cmd(0x15).await?; // Set column address
		self.send_cmd(0x00).await?; // Start column
		self.send_cmd(0x7F).await?; // End column (127)

		self.send_cmd(0x75).await?; // Set row address
		self.send_cmd(0x00).await?; // Start row
		self.send_cmd(0x3F).await?; // End row (63)

		self.send_cmd(0x81).await?; // Set contrast for color
		self.send_cmd(0xFF).await?; // Contrast value

		self.send_cmd(0xA0).await?; // Set remap and data format
		self.send_cmd(if flip { 0b01010010 } else { 0b11000011 })
			.await?;

		self.send_cmd(0xA1).await?; // Set display start line
		self.send_cmd(0x00).await?;

		self.send_cmd(0xA2).await?; // Set display offset
		self.send_cmd(0x00).await?;

		self.send_cmd(0xA4).await?; // Normal display mode

		self.send_cmd(0xA8).await?; // Set multiplex ratio
		self.send_cmd(0x3F).await?; // 1/64 duty

		self.send_cmd(0xAB).await?; // Set VDD internal regulator
		self.send_cmd(0x01).await?; // Enable

		self.send_cmd(0xAD).await?; // External / Internal IREF selection
		self.send_cmd(0x8E).await?; // Enable internal IREF

		self.send_cmd(0xB1).await?; // Set phase length
		self.send_cmd(0x22).await?;

		self.send_cmd(0xB3).await?; // Set display clock divide ratio/oscillator frequency
		self.send_cmd(0xF0).await?; // Highest clock ratio (0x#_) / lowest divide ratio (0x_#)

		self.send_cmd(0xB6).await?; // Set second precharge period
		self.send_cmd(0x04).await?;

		self.send_cmd(0xB9).await?; // Set linear LUT

		self.send_cmd(0xBC).await?; // Set precharge voltage
		self.send_cmd(0x10).await?; // 0.5 x Vcc

		self.send_cmd(0xBD).await?; // Pre-charge voltage capacitor selection
		self.send_cmd(0x01).await?;

		self.send_cmd(0xBE).await?; // Set COM deselect voltage
		self.send_cmd(0x07).await?; // 0.82 x Vcc

		self.send_cmd(0xAF).await?; // Display on

		Ok(())
	}

	async fn display_on(&mut self) -> Result<(), Error> {
		self.send_cmd(0xAF).await // Display on
	}

	async fn display_off(&mut self) -> Result<(), Error> {
		self.send_cmd(0xAE).await // Display off
	}

	async fn set_contrast(&mut self, contrast: u8) -> Result<(), Error> {
		let curve_index = (u16::from(contrast) * 63 / 255) as usize;
		let curved_contrast = BRIGHTNESS_CURVE[curve_index];
		self.set_contrast_raw(curved_contrast).await
	}

	async fn set_contrast_raw(&mut self, contrast: u8) -> Result<(), Error> {
		self.send_cmd(0x81).await?; // Set contrast for color
		self.send_cmd(contrast).await?;
		Ok(())
	}

	async fn reset_cursor(&mut self) -> Result<(), Error> {
		self.send_cmd(0x15).await?; // Set column address
		self.send_cmd(0x00).await?; // Start column
		self.send_cmd(0x7F).await?; // End column (127)

		self.send_cmd(0x75).await?; // Set row address
		self.send_cmd(0x00).await?; // Start row
		self.send_cmd(0x3F).await?; // End row (63)
		Ok(())
	}

	async fn write_checkers_debug(&mut self) -> Result<(), Error> {
		self.reset_cursor().await?;
		let mut buf = [0u8; 64 * 128];
		buf.iter_mut().step_by(2).for_each(|b| *b = 0xFF);
		self.send_data(&buf).await?;
		Ok(())
	}

	async fn repaint(&mut self) -> Result<(), Error> {
		if !self.comms_enabled {
			return Ok(());
		}

		self.reset_cursor().await?;
		// Due to the borrow checker we have to perform the write directly
		self.dc.set_high(); // data
		self.cs.set_low();
		let data = self.framebuf.data();
		let r = self.spi.write(data).await;
		// Always flush even if write fails
		let fr = self.flush().await;
		self.cs.set_high();
		r?;
		fr?;
		Ok(())
	}
}

impl OriginDimensions for SSD1362 {
	fn size(&self) -> Size {
		(256, 64).into()
	}
}

impl DrawTarget for SSD1362 {
	type Color = <FrameBuf as DrawTarget>::Color;
	type Error = <FrameBuf as DrawTarget>::Error;

	fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
	where
		I: IntoIterator<Item = Pixel<Self::Color>>,
	{
		FrameBuf::draw_iter::<I>(&mut self.framebuf, pixels)
	}

	fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
	where
		I: IntoIterator<Item = Self::Color>,
	{
		FrameBuf::fill_contiguous::<I>(&mut self.framebuf, area, colors)
	}

	fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
		FrameBuf::fill_solid(&mut self.framebuf, area, color)
	}

	fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
		FrameBuf::clear(&mut self.framebuf, color)
	}
}
