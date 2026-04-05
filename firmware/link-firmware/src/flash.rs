// SAFETY: These symbols are defined in memory.x

use core::mem::{ManuallyDrop, MaybeUninit};

use embassy_stm32::{
	Peri,
	flash::{Blocking, Flash, FlashLayout},
	peripherals,
};
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use static_cell::StaticCell;

use crate::crc32::Crc32Ext;

unsafe extern "C" {
	static _persistent_flash_start: u8;
	static _persistent_flash_end: u8;
}

const COOKIE: u32 = 0x4F50464C; // "OPFL"

pub type Pflash = PflashV0;

#[derive(defmt::Format)]
pub enum PflashVersion {
	V0(PflashV0),
}

impl PflashVersion {
	/// Always returns the latest version of the persistent flash data.
	pub fn into_latest(self) -> Pflash {
		match self {
			PflashVersion::V0(v0) => v0,
		}
	}
}

impl Default for PflashVersion {
	fn default() -> Self {
		PflashVersion::V0(Default::default())
	}
}

#[derive(defmt::Format, Clone)]
#[repr(C)]
pub struct PflashV0 {
	/// Whether not the system has been initialized.
	pub initialized: bool,
}

impl Default for PflashV0 {
	fn default() -> Self {
		PflashV0 { initialized: false }
	}
}

impl From<PflashV0> for PflashVersion {
	fn from(v0: PflashV0) -> Self {
		PflashVersion::V0(v0)
	}
}

#[repr(C)]
struct PflashData {
	pub crc:     u32,
	pub cookie:  u32,
	pub version: u32,
	// MUST BE LAST.
	pub data:    PflashUnion,
}

#[repr(C)]
union PflashUnion {
	pub v0: ManuallyDrop<PflashV0>,
}

#[derive(defmt::Format, Debug)]
pub enum Error {
	BadCrc,
	BadCookie(u32),
	UnsupportedVersion(u32),
	Flash(embassy_stm32::flash::Error),
}

impl From<embassy_stm32::flash::Error> for Error {
	#[inline]
	fn from(err: embassy_stm32::flash::Error) -> Self {
		Error::Flash(err)
	}
}

static PFLASH: StaticCell<FlashLayout<'static, Blocking>> = StaticCell::new();
static mut PFLASH_INST: Option<&'static mut FlashLayout<'static, Blocking>> = None;

pub fn init_pflash(flash_peripheral: Peri<'static, peripherals::FLASH>) {
	let flash = PFLASH.init(Flash::new_blocking(flash_peripheral).into_blocking_regions());

	// SAFETY: We ensure that PFLASH_INST is only written once here during initialization.
	// SAFETY: We can guarantee that as `PFLASH.init()` would otherwise panic.
	unsafe {
		PFLASH_INST = Some(flash);
	}

	// Now make sure that the bank size fits the erase size.
	// SAFETY: We just initialized the flash.
	let flash = unsafe { get_flash() };

	let flash_size = pflash_size();
	let (region, offset) = get_pflash_bank_and_offset(flash);

	defmt::debug!(
		"persistent flash size: {} bytes, offset {:08X}, region erase size {} bytes",
		flash_size,
		offset,
		region.erase_size()
	);

	let mut success = true;
	if let Err(err) = region.check_read(offset, core::mem::size_of::<PflashData>()) {
		defmt::error!(
			"persistent flash region is not readable given size/length: {}",
			err.as_str()
		);
		success = false;
	}
	if let Err(err) = region.check_write(offset, core::mem::size_of::<PflashData>()) {
		defmt::error!(
			"persistent flash region is not writable given size/length: {}",
			err.as_str()
		);
		success = false;
	}
	if let Err(err) = region.check_erase(offset, offset + flash_size) {
		defmt::error!(
			"persistent flash region is not erasable given size/length: {}",
			err.as_str()
		);
		success = false;
	}
	if !success {
		panic!("persistent flash region is not usable");
	}
}

/// # Safety
/// Caller must ensure this is only being used in a single-threaded context.
unsafe fn get_flash() -> &'static mut FlashLayout<'static, Blocking> {
	// SAFETY: We ensure that PFLASH_INST is initialized before use in `init_pflash`.
	#[expect(static_mut_refs)]
	unsafe {
		PFLASH_INST
			.as_mut()
			.expect("Persistent flash not initialized")
	}
}

fn pflash_size() -> u32 {
	// SAFETY: We are just reading linker-defined symbols.
	unsafe {
		debug_assert_eq!(
			core::mem::size_of::<*const u8>(),
			4,
			"unexpected pointer size"
		);
		let start = &_persistent_flash_start as *const u8 as u32;
		let end = &_persistent_flash_end as *const u8 as u32;
		end - start
	}
}

trait NorFlashAsStr {
	fn as_str(&self) -> &str;
}

impl NorFlashAsStr for embedded_storage::nor_flash::NorFlashErrorKind {
	fn as_str(&self) -> &str {
		use embedded_storage::nor_flash::NorFlashErrorKind;

		match self {
			NorFlashErrorKind::OutOfBounds => "OutOfBounds",
			NorFlashErrorKind::Other => "Other",
			NorFlashErrorKind::NotAligned => "NotAligned",
			_ => "(???)",
		}
	}
}

trait FlashRegion {
	fn write(&mut self, offset: u32, data: &[u8]) -> Result<(), embassy_stm32::flash::Error>;
	fn read(&mut self, offset: u32, buf: &mut [u8]) -> Result<(), embassy_stm32::flash::Error>;
	fn erase(&mut self, offset: u32, len: u32) -> Result<(), embassy_stm32::flash::Error>;
	fn check_write(
		&self,
		offset: u32,
		len: usize,
	) -> Result<(), embedded_storage::nor_flash::NorFlashErrorKind>;
	fn check_read(
		&self,
		offset: u32,
		len: usize,
	) -> Result<(), embedded_storage::nor_flash::NorFlashErrorKind>;
	fn check_erase(
		&self,
		offset: u32,
		to: u32,
	) -> Result<(), embedded_storage::nor_flash::NorFlashErrorKind>;
	fn erase_size(&self) -> usize;
}

impl<T: NorFlash<Error = embassy_stm32::flash::Error>> FlashRegion for T {
	fn write(&mut self, offset: u32, data: &[u8]) -> Result<(), embassy_stm32::flash::Error> {
		<Self as NorFlash>::write(self, offset, data)
	}

	fn read(&mut self, offset: u32, buf: &mut [u8]) -> Result<(), embassy_stm32::flash::Error> {
		<Self as ReadNorFlash>::read(self, offset, buf)
	}

	fn erase(&mut self, offset: u32, len: u32) -> Result<(), embassy_stm32::flash::Error> {
		<Self as NorFlash>::erase(self, offset, len)
	}

	fn check_write(
		&self,
		offset: u32,
		len: usize,
	) -> Result<(), embedded_storage::nor_flash::NorFlashErrorKind> {
		embedded_storage::nor_flash::check_write(self, offset, len)
	}

	fn check_read(
		&self,
		offset: u32,
		len: usize,
	) -> Result<(), embedded_storage::nor_flash::NorFlashErrorKind> {
		embedded_storage::nor_flash::check_read(self, offset, len)
	}

	fn check_erase(
		&self,
		offset: u32,
		to: u32,
	) -> Result<(), embedded_storage::nor_flash::NorFlashErrorKind> {
		embedded_storage::nor_flash::check_erase(self, offset, to)
	}

	fn erase_size(&self) -> usize {
		<Self as NorFlash>::ERASE_SIZE
	}
}

fn get_pflash_bank_and_offset(
	flash: &'static mut FlashLayout<'static, Blocking>,
) -> (&'static mut dyn FlashRegion, u32) {
	debug_assert_eq!(
		core::mem::size_of::<*const u8>(),
		4,
		"unexpected pointer size"
	);

	defmt::trace!("fetching pflash bank and offset");

	// SAFETY: We just checked the pointer size and know that these linker symbols exist.
	let (pflash_base, pflash_end) = unsafe {
		(
			&_persistent_flash_start as *const u8 as u32,
			&_persistent_flash_end as *const u8 as u32,
		)
	};

	defmt::trace!("pflash base: {:08X}, end: {:08X}", pflash_base, pflash_end);

	macro_rules! check_region {
		($region:expr, $name:expr) => {
			defmt::trace!("checking region {}", $name);

			if $region.0.base() <= pflash_base && $region.0.base() + $region.0.size >= pflash_end {
				let base = $region.0.base();
				let offset = pflash_base - base;
				defmt::debug!(
					"found pflash region in {} at base {:08X} offset {}",
					$name,
					base,
					offset
				);
				return (&mut $region, offset);
			}
		};
	}

	// Try to find the region that contains the persistent flash area.
	check_region!(flash.bank1_region1, "bank 1, region 1");
	check_region!(flash.bank1_region2, "bank 1, region 2");
	check_region!(flash.bank1_region3, "bank 1, region 3");

	panic!("could not find persistent flash region");
}

impl PflashData {
	fn calculate_crc(&self) -> Result<u32, Error> {
		let mut crc_calc = crc32fast::Hasher::new();
		crc_calc.update(&self.cookie.to_le_bytes());
		crc_calc.update(&self.version.to_le_bytes());
		match self.version {
			0 => unsafe { self.data.v0.crc32_into(&mut crc_calc) },
			_ => return Err(Error::UnsupportedVersion(self.version)),
		}

		Ok(crc_calc.finalize())
	}
}

fn read_raw_pflash() -> Result<PflashData, Error> {
	assert!(
		pflash_size() >= core::mem::size_of::<PflashVersion>().try_into().unwrap(),
		"persistent flash size is too small"
	);

	defmt::debug!("reading pflash");

	// SAFETY: This function is blocking.
	let flash = unsafe { get_flash() };
	let (region, offset) = get_pflash_bank_and_offset(flash);

	let mut raw_data = MaybeUninit::<PflashData>::uninit();
	// SAFETY: We've performed the necessary size checks above.
	region.read(offset, unsafe {
		core::slice::from_raw_parts_mut(
			raw_data.as_mut_ptr() as *mut u8,
			core::mem::size_of::<PflashData>(),
		)
	})?;

	// SAFETY: We perform additional checks below.
	let data = unsafe { raw_data.assume_init() };

	defmt::debug!(
		"read pflash data: crc={:08X}, cookie={:08X}, version={}",
		data.crc,
		data.cookie,
		data.version
	);

	// Perform cookie check.
	if data.cookie != COOKIE {
		defmt::warn!(
			"pflash cookie mismatch: expected {:08X}, got {:08X}",
			COOKIE,
			data.cookie
		);
		return Err(Error::BadCookie(data.cookie));
	}

	// Perform version + CRC check
	let crc = data.calculate_crc()?;
	if crc != data.crc {
		defmt::warn!(
			"pflash CRC mismatch: expected {:08X}, got {:08X}",
			data.crc,
			crc
		);
		return Err(Error::BadCrc);
	}

	defmt::trace!(
		"pflash read successful: version={} cookie={:08X} crc={:08X}",
		data.version,
		data.cookie,
		crc
	);

	Ok(data)
}

/// # Safety
/// Version MUST be valid.
unsafe fn into_pflash(data: PflashData) -> PflashVersion {
	match data.version {
		0 => {
			let v0 = unsafe { ManuallyDrop::into_inner(data.data.v0) };
			PflashVersion::V0(v0)
		}
		_ => unreachable!(),
	}
}

pub fn read_pflash() -> Result<PflashVersion, Error> {
	let data = read_raw_pflash()?;
	// SAFETY: Version has already been validated in `read_raw_pflash`.
	Ok(unsafe { into_pflash(data) })
}

/// # Safety
/// **This is a very, very slow, _blocking_ operation**, and will **wear down
/// the flash memory** if used frequently. Caller MUST ensure that this is only
/// used in a safe context.
pub unsafe fn write_pflash(
	pflash: impl Into<PflashVersion> + defmt::Format,
) -> Result<PflashVersion, Error> {
	assert!(
		pflash_size() >= core::mem::size_of::<PflashVersion>().try_into().unwrap(),
		"persistent flash size is too small"
	);

	defmt::debug!("writing pflash: {:?}", pflash);

	let pflash = pflash.into();
	defmt::trace!("converted flash data: {:?}", pflash);

	// SAFETY: This function is blocking.
	let flash = unsafe { get_flash() };
	defmt::trace!("got flash handle");

	let (region, offset) = get_pflash_bank_and_offset(flash);
	defmt::trace!("got pflash region and offset: offset={:08X}", offset);

	// Prepare raw data.
	let mut data = PflashData {
		crc:     0,
		cookie:  COOKIE,
		version: match pflash {
			PflashVersion::V0(_) => 0,
		},
		data:    match pflash {
			PflashVersion::V0(v0) => {
				PflashUnion {
					v0: ManuallyDrop::new(v0),
				}
			}
		},
	};
	defmt::trace!("prepared raw pflash data");

	// Calculate CRC.
	// We can unwrap here since we just constructed the data
	// and can guarantee the version is valid.
	data.crc = data.calculate_crc().unwrap();
	defmt::trace!("calculated pflash CRC: {:08X}", data.crc);

	// Erase
	let flash_size = pflash_size();
	defmt::debug!(
		"performing pflash erase (from {:08X} to {:08X})",
		offset,
		offset + flash_size
	);
	region.erase(offset, offset + flash_size)?;

	// Write
	defmt::debug!(
		"performing pflash write (size = {})",
		core::mem::size_of::<PflashData>()
	);
	region.write(offset, unsafe {
		core::slice::from_raw_parts(
			&data as *const PflashData as *const u8,
			core::mem::size_of::<PflashData>(),
		)
	})?;

	defmt::debug!(
		"pflash write complete (crc = {:08X}); performing readback",
		data.crc
	);

	// Read back to verify.
	let pflash_readback = read_raw_pflash()?;
	if pflash_readback.crc != data.crc {
		defmt::warn!(
			"pflash readback CRC mismatch: expected {:08X}, got {:08X}",
			data.crc,
			pflash_readback.crc
		);
		return Err(Error::BadCrc);
	}

	// SAFETY: Version has already been validated in `read_raw_pflash`.
	Ok(unsafe { into_pflash(pflash_readback) })
}
