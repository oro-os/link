use core::sync::atomic::AtomicU32;

use embassy_stm32::{
	gpio::{Output, OutputOpenDrain},
	mode::Async,
	spi::{Error, Spi},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};
use embedded_graphics::{
	framebuffer::{Framebuffer, buffer_size},
	pixelcolor::{Gray4, PixelColor, raw::LittleEndian},
};
use embedded_graphics_core::geometry::{OriginDimensions, Size};
use embedded_hal_async::spi::SpiBus;

use crate::atomic::NumericRelaxed;

const BRIGHTNESS_CURVE: [u8; 64] = [
	0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 8, 8, 9, 10, 11, 12, 12, 13,
	14, 15, 16, 17, 18, 19, 21, 22, 23, 24, 25, 27, 28, 29, 31, 32, 34, 35, 37, 38, 40, 41, 43, 45,
	46, 48, 50, 52, 53, 55, 57, 59, 61, 63,
];

pub type Channel = crate::channel::Channel<Cmd, 4>;

pub type FrameBuf = Framebuffer<
	Gray4,
	<Gray4 as PixelColor>::Raw,
	LittleEndian,
	256,
	64,
	{ buffer_size::<Gray4>(256, 64) },
>;

pub static FRAME_BUFFER: Mutex<CriticalSectionRawMutex, FrameBuf> = Mutex::new(FrameBuf::new());
pub static FRAME_COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(defmt::Format)]
#[allow(unused)]
pub enum Cmd {
	SetPower { enabled: bool },
	Render,
	SetBrightness { brightness: u8 },
}

pub struct Config {
	pub spi:     Spi<'static, Async>,
	pub cs:      OutputOpenDrain<'static>,
	pub dc:      Output<'static>,
	pub rst:     Output<'static>,
	pub vreg_en: Output<'static>,
}

#[embassy_executor::task]
pub async fn run(rx: &'static Channel, config: Config) -> ! {
	let Config {
		spi,
		cs,
		dc,
		mut rst,
		mut vreg_en,
	} = config;

	let mut oled = Ssd1362::new(spi, dc, cs);
	let mut vbus_state = false;

	defmt::trace!("beginning main loop");
	loop {
		match rx.receive().await {
			Cmd::SetPower { enabled } => {
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
						let fb = FRAME_BUFFER.lock().await;
						oled.repaint(&fb).await.expect("OLED repaint failed");
						drop(fb);
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
			Cmd::SetBrightness { brightness } => {
				oled.set_contrast(brightness).await.unwrap();
			}
			Cmd::Render => {
				oled.repaint(&*FRAME_BUFFER.lock().await).await.unwrap();
				FRAME_COUNTER.increment();
			}
		}
	}
}

struct Ssd1362 {
	spi: Spi<'static, Async>,
	dc: Output<'static>,
	cs: OutputOpenDrain<'static>,
	comms_enabled: bool,
}

#[expect(dead_code)]
impl Ssd1362 {
	fn new(spi: Spi<'static, Async>, dc: Output<'static>, cs: OutputOpenDrain<'static>) -> Self {
		Self {
			spi,
			dc,
			cs,
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

	async fn repaint(&mut self, framebuf: &FrameBuf) -> Result<(), Error> {
		if !self.comms_enabled {
			return Ok(());
		}

		self.reset_cursor().await?;
		// Due to the borrow checker we have to perform the write directly
		self.dc.set_high(); // data
		self.cs.set_low();
		let data = framebuf.data();
		let r = self.spi.write(data).await;
		// Always flush even if write fails
		let fr = self.flush().await;
		self.cs.set_high();
		r?;
		fr?;
		Ok(())
	}
}

impl OriginDimensions for Ssd1362 {
	fn size(&self) -> Size {
		(256, 64).into()
	}
}
