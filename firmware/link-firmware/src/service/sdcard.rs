use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_stm32::{
	exti::ExtiInput,
	gpio::{Output, OutputOpenDrain},
	mode::Async,
	spi::Spi,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};

#[embassy_executor::task]
pub async fn sdcard_service(
	sd: &'static Mutex<NoopRawMutex, Spi<'static, Async>>,
	mut sd_cs: OutputOpenDrain<'static>,
	mut sd_en: OutputOpenDrain<'static>,
	mut sd_oc: ExtiInput<'static>,
	mut sd_sense: ExtiInput<'static>,
	mut sd_sense_cable: ExtiInput<'static>,
	mut sd_host_sut_sel: Output<'static>,
) -> ! {
	defmt::info!("switching SD card to Host mode...");
	sd_host_sut_sel.select_host();
	Timer::after(Duration::from_millis(10)).await;

	loop {
		sd_en.set_high(); // Turn off power
		sd_cs.set_high(); // Deselect SD card
		Timer::after(Duration::from_secs(2)).await;
		if sd_sense.is_low() {
			defmt::info!("no SD card inserted");
			continue;
		}

		defmt::info!("SD card inserted; powering up...");
		sd_en.set_low(); // Turn on power
		Timer::after(Duration::from_millis(100)).await;

		defmt::info!("initializing SD card...");
		let mut card = SdCard {
			sd,
			sd_cs: &mut sd_cs,
		};

		match card.init().await {
			Ok(()) => {
				defmt::info!("SD card initialized successfully");
			}
			Err(e) => {
				defmt::warn!("SD card initialization failed: {:?}", e);
			}
		}
	}
}

struct SdCard<'a> {
	sd: &'a Mutex<NoopRawMutex, Spi<'static, Async>>,
	sd_cs: &'a mut OutputOpenDrain<'static>,
}

#[derive(defmt::Format)]
enum SdCardError {
	Spi(embassy_stm32::spi::Error),
	NotIdle,
	NoResponse,
}

#[derive(defmt::Format)]
enum SdCardType {
	SDv1,
	SDv2,
	SDHC,
}

impl<'a> SdCard<'a> {
	async fn init(&mut self) -> Result<(), SdCardError> {
		// CMD0: GO_IDLE_STATE
		self.cmd0().await?;

		// CMD8: SEND_IF_COND (determine card type + echo back verification)
		let card_type = self.cmd8().await?;
		defmt::debug!("SD card type: {:?}", card_type);

		//

		Ok(())
	}

	/// CMD8: SEND_IF_COND
	///
	/// **Note:** This command will NEVER return [`SdCardType::SDHC`] directly.
	/// To determine if the card is SDHC, you need to send ACMD41 afterwards
	/// and check the CCS bit in the OCR register.
	async fn cmd8(&mut self) -> Result<SdCardType, SdCardError> {
		let r7 = self.send_r7_command(8, 0x000001AA, 0x87).await?;

		if r7.r1.is_illegal_command() {
			// SDv1 or not SD card
			defmt::trace!("CMD8: illegal command; assuming SDv1");
			return Ok(SdCardType::SDv1);
		}

		if !r7.r1.is_idle() {
			defmt::warn!("CMD8: card not in idle state: {:?}", r7.r1);
			return Err(SdCardError::NotIdle);
		}

		if (r7.echo_back & 0xFFF) != 0x1AA {
			defmt::warn!("CMD8: invalid echo back: {:X}", r7.echo_back);
			return Err(SdCardError::NoResponse);
		}

		// SDv2
		defmt::trace!("CMD8: SDv2+ card detected");
		Ok(SdCardType::SDv2)
	}

	/// Sends an R1 command and returns the R1 response.
	async fn send_r1_command(&mut self, cmd: u8, arg: u32, crc: u8) -> Result<R1, SdCardError> {
		let mut r = [0xFFu8; 6 + 16]; // Command + response + padding
		debug_assert_eq!(cmd & 0xC0, 0, "invalid command index");
		r[0] = 0x40 | cmd; // Command index
		r[1] = ((arg >> 24) & 0xFF) as u8; // Argument[31:24]
		r[2] = ((arg >> 16) & 0xFF) as u8; // Argument[23:16]
		r[3] = ((arg >> 8) & 0xFF) as u8; // Argument[15:8]
		r[4] = (arg & 0xFF) as u8; // Argument[7:0]
		r[5] = crc; // CRC

		defmt::trace!("CMD{}: sending: {:X}", cmd, &r[..6]);
		{
			// We take a lease out on the SPI bus for the duration of the command.
			let mut sd = self.sd.lock().await;

			self.sd_cs.set_low(); // Assert
			Timer::after(Duration::from_micros(10)).await;
			sd.transfer_in_place(&mut r).await?;
			self.sd_cs.set_high(); // De-assert
			Timer::after(Duration::from_micros(10)).await;
		}

		defmt::trace!("CMD{}: response: {:X}", cmd, &r[6..]);

		// Find the first response byte (MSB=0)
		for &b in &r[6..] {
			if b & 0x80 == 0 {
				return Ok(R1(b));
			}
		}

		Err(SdCardError::NoResponse)
	}

	/// Sends an R7 command and returns the R1 response.
	async fn send_r7_command(&mut self, cmd: u8, arg: u32, crc: u8) -> Result<R7, SdCardError> {
		let mut r = [0xFFu8; 6 + 16]; // Command + response + padding
		debug_assert_eq!(cmd & 0xC0, 0, "invalid command index");
		r[0] = 0x40 | cmd; // Command index
		r[1] = ((arg >> 24) & 0xFF) as u8; // Argument[31:24]
		r[2] = ((arg >> 16) & 0xFF) as u8; // Argument[23:16]
		r[3] = ((arg >> 8) & 0xFF) as u8; // Argument[15:8]
		r[4] = (arg & 0xFF) as u8; // Argument[7:0]
		r[5] = crc; // CRC

		defmt::trace!("CMD{}: sending: {:X}", cmd, &r[..6]);
		{
			// We take a lease out on the SPI bus for the duration of the command.
			let mut sd = self.sd.lock().await;

			self.sd_cs.set_low(); // Assert
			Timer::after(Duration::from_micros(10)).await;
			sd.transfer_in_place(&mut r).await?;
			self.sd_cs.set_high(); // De-assert
			Timer::after(Duration::from_micros(10)).await;
		}

		defmt::trace!("CMD{}: response: {:X}", cmd, &r[6..]);

		// Find the first response byte (MSB=0)
		let mut found = false;
		let mut i = 0;
		for j in 6..r.len() {
			if r[j] & 0x80 == 0 {
				found = true;
				i = j;
				break;
			}
		}

		if !found || i > r.len() - 5 {
			defmt::warn!("CMD{}: no valid response found", cmd);
			return Err(SdCardError::NoResponse);
		}

		let r1 = R1(r[i]);
		let r7_bytes = &r[(i + 1)..(i + 5)];
		let r7 = R7 {
			r1,
			echo_back: ((r7_bytes[0] as u32) << 24)
				| ((r7_bytes[1] as u32) << 16)
				| ((r7_bytes[2] as u32) << 8)
				| (r7_bytes[3] as u32),
		};

		Ok(r7)
	}

	/// CMD0: GO_IDLE_STATE
	async fn cmd0(&mut self) -> Result<(), SdCardError> {
		// We take a lease out on the SPI bus for the duration of CMD0.
		let mut sd = self.sd.lock().await;

		// Send dummy clocks
		self.sd_cs.set_high(); // De-assert
		Timer::after(Duration::from_micros(10)).await;
		sd.write(&[0xFFu8; 10]).await?;

		// Send CMD0
		self.sd_cs.set_low(); // Assert
		Timer::after(Duration::from_micros(10)).await;

		let mut r = [0xFFu8; 24]; // 16 bytes in the response, just in case.
		r[0] = 0x40 | 0x00; // Command index
		r[1] = 0x00; // Argument[31:24]
		r[2] = 0x00; // Argument[23:16]
		r[3] = 0x00; // Argument[15:8]
		r[4] = 0x00; // Argument[7:0]
		r[5] = 0x95; // CRC

		defmt::trace!("CMD0: sending: {:X}", &r[..6]);
		sd.transfer_in_place(&mut r).await?;
		self.sd_cs.set_high(); // De-assert
		Timer::after(Duration::from_micros(10)).await;
		defmt::trace!("CMD0: response: {:X}", &r[6..]);

		// Do *any* of the response bytes indicate success
		// (and are followed by all 0xFF bytes)?
		//
		// Why do this? Because not all cards immediately switch to
		// SPI mode and thus have a drained MISO line (even though
		// we pull it up) for the first few _bits_.
		//
		// For CMD0 specifically, we wait for a full `0x01` response
		// followed by 0 or more `0xFF` bytes to be the most robust
		// implementation of "go idle state". This isn't technically
		// to spec, but it works in practice.
		//
		// Why not wait between the CMD0 and reading the response?
		// Because some cards do not like when SCK is idle for too long
		// between bytes in a single transaction.
		let mut i = 0;
		let mut found = false;
		while i < r.len() {
			if r[i] & 0x80 == 0 {
				// Found a response byte
				if r[i] == 0x01 {
					found = true;
					break;
				}
			}
			i += 1;
		}

		if !found {
			defmt::warn!("CMD0: no valid response found");
			return Err(SdCardError::NotIdle);
		}

		if found {
			// Make sure all subsequent bytes are 0xFF
			for j in (i + 1)..r.len() {
				if r[j] != 0xFF {
					defmt::warn!("CMD0: invalid trailing byte: {:02X}", r[j]);
					return Err(SdCardError::NotIdle);
				}
			}
		}

		Ok(())
	}
}

impl From<embassy_stm32::spi::Error> for SdCardError {
	#[inline]
	fn from(err: embassy_stm32::spi::Error) -> Self {
		SdCardError::Spi(err)
	}
}

trait HostSutSelect {
	fn select_host(&mut self);
	fn select_sut(&mut self);
}

impl HostSutSelect for Output<'static> {
	fn select_host(&mut self) {
		self.set_low();
	}

	fn select_sut(&mut self) {
		self.set_high();
	}
}

#[derive(defmt::Format)]
#[repr(transparent)]
#[derive(Copy, Clone)]
struct R1(u8);

impl R1 {
	const fn is_zero(&self) -> bool {
		self.0 == 0
	}

	const fn is_idle(&self) -> bool {
		(self.0 & (1 << 0)) != 0
	}

	const fn is_erase_reset(&self) -> bool {
		(self.0 & (1 << 1)) != 0
	}

	const fn is_illegal_command(&self) -> bool {
		(self.0 & (1 << 2)) != 0
	}

	const fn is_com_crc_error(&self) -> bool {
		(self.0 & (1 << 3)) != 0
	}

	const fn is_erasure_sequence_error(&self) -> bool {
		(self.0 & (1 << 4)) != 0
	}

	const fn is_address_error(&self) -> bool {
		(self.0 & (1 << 5)) != 0
	}

	const fn is_parameter_error(&self) -> bool {
		(self.0 & (1 << 6)) != 0
	}
}

#[derive(defmt::Format)]
struct R7 {
	r1: R1,
	echo_back: u32,
}
