use embassy_stm32::gpio::{Output, OutputOpenDrain};
use embassy_stm32::spi::Error;
use embassy_stm32::{mode::Async, spi::Spi};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Delay;
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
use embedded_hal_bus::spi::ExclusiveDevice;
use micromath::F32Ext;

#[embassy_executor::task]
pub async fn oled_service(
	mut spi: Spi<'static, Async>,
	mut cs: OutputOpenDrain<'static>,
	mut dc: Output<'static>,
	mut rst: Output<'static>,
	mut vreg_en: Output<'static>,
) -> ! {
	defmt::info!("enabling OLED power regulator...");
	vreg_en.set_high();
	Timer::after(Duration::from_millis(10)).await;

	defmt::info!("resetting OLED...");
	rst.set_low();
	Timer::after(Duration::from_millis(10)).await;
	rst.set_high();
	Timer::after(Duration::from_millis(10)).await;

	defmt::info!("initializing OLED...");

	let mut oled = SSD1362::new(spi, dc, cs);
	oled.init(true).await.unwrap();
	defmt::info!("OLED initialized");

	oled.repaint().await.unwrap();

	oled.clear(embedded_graphics::pixelcolor::Gray4::new(0))
		.unwrap();
	for x in 0..256 {
		for y in 0..64 {
			let v = ((x as f32 / 8.0).sin() + (y as f32 / 8.0).cos()) * 0.5 + 0.5;
			let gray = (v * 15.0) as u8;
			oled.framebuf.set_pixel(
				Point::new(x as i32, y as i32),
				embedded_graphics::pixelcolor::Gray4::new(gray),
			);
		}
	}
	oled.repaint().await.unwrap();
	loop {
		Timer::after(Duration::from_secs(1)).await;
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
}

impl SSD1362 {
	fn new(spi: Spi<'static, Async>, dc: Output<'static>, cs: OutputOpenDrain<'static>) -> Self {
		Self {
			spi,
			dc,
			cs,
			framebuf: Framebuffer::new(),
		}
	}

	async fn flush(&mut self) -> Result<(), Error> {
		<Spi<'static, Async> as SpiBus<u8>>::flush(&mut self.spi).await?;
		Ok(())
	}

	async fn send_cmd(&mut self, cmd: u8) -> Result<(), Error> {
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
		self.send_cmd(0xF0 | 0x00).await?; // Highest clock ratio (0x#_) / lowest divide ratio (0x_#)

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
