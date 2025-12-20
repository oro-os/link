use core::mem::MaybeUninit;

use embassy_stm32::pac;

use crate::crc32::Crc32Ext;

const COOKIE: u32 = 0x1337BABE;
const VERSION: u32 = 1;

#[unsafe(link_section = ".bkpsram")]
static mut NV_RAM: MaybeUninit<NvRam> = MaybeUninit::uninit();
static mut NV_INITIALIZED_THIS_SESSION: bool = false;

#[derive(Default)]
pub struct Volatile<T>(T);

impl<T> defmt::Format for Volatile<T>
where
	T: defmt::Format + Copy,
{
	fn format(&self, fmt: defmt::Formatter) {
		defmt::write!(fmt, "{:?}", self.read());
	}
}

impl<T> Volatile<T> {
	pub fn read(&self) -> T
	where
		T: Copy,
	{
		// SAFETY: These are all coming from refs; they're safe to call.
		unsafe { core::ptr::read_volatile(&self.0) }
	}

	pub fn write(&mut self, val: T)
	where
		T: Copy,
	{
		// SAFETY: These are all coming from refs; they're safe to call.
		unsafe { core::ptr::write_volatile(&mut self.0, val) }
	}
}

#[derive(defmt::Format)]
#[repr(C)]
pub struct NvRam {
	integrity:  Integrity,
	pub reboot: NvRamRebootStats,
}

impl NvRam {
	pub fn reset(&mut self) {
		self.integrity.reset();
		self.reboot.reset();
	}
}

#[derive(defmt::Format)]
#[repr(C)]
pub struct NvRamRebootStats {
	pub fast_count:  Volatile<u32>,
	pub in_progress: Volatile<bool>,
}

impl NvRamRebootStats {
	pub fn reset(&mut self) {
		self.fast_count.write(0);
		self.in_progress.write(false);
	}
}

#[derive(defmt::Format)]
#[repr(C)]
struct Integrity {
	crc:  Volatile<u32>,
	data: Volatile<IntegrityData>,
}

impl Integrity {
	fn check(&self) -> bool {
		let idata = self.data.read();
		let crc = self.crc.read();
		crc == idata.crc32()
			&& idata.cookie == COOKIE
			&& idata.version == VERSION
			&& idata.sizeof == core::mem::size_of::<NvRam>()
	}
}

impl Integrity {
	fn reset(&mut self) {
		self.data.write(IntegrityData::new_without_crc32());
		self.crc.write(self.data.read().crc32());
	}
}

#[derive(defmt::Format, Clone, Copy)]
#[repr(C)]
struct IntegrityData {
	cookie:  u32,
	version: u32,
	sizeof:  usize,
	nonce:   u32,
}

impl IntegrityData {
	fn new_without_crc32() -> Self {
		Self {
			cookie:  COOKIE,
			version: VERSION,
			sizeof:  core::mem::size_of::<NvRam>(),
			nonce:   crate::rand::next_u32(),
		}
	}
}

#[expect(static_mut_refs)]
pub fn init() -> &'static mut NvRam {
	unsafe {
		if NV_INITIALIZED_THIS_SESSION {
			panic!("NvRam initialized more than once this session");
		}

		NV_INITIALIZED_THIS_SESSION = true;
	}

	defmt::trace!("initializing NVRAM");

	// Enable BKPSRAM
	{
		let rcc = pac::RCC;
		let pwr = pac::PWR;

		// Step 1: Enable PWR clock
		defmt::trace!("enabling PWR clock");
		rcc.apb1enr().modify(|w| w.set_pwren(true));

		// Step 2: Enable backup domain write access
		defmt::trace!("enabling backup domain access");
		pwr.cr1().modify(|w| w.set_dbp(true));

		// Step 3: Enable backup SRAM clock
		defmt::trace!("enabling backup SRAM clock");
		rcc.ahb1enr().modify(|w| w.set_bkpsramen(true));
	}

	defmt::trace!("NVRAM registers initialized");

	// SAFETY: This is technically UB. However we properly handle
	// integrity checking and initialization below.
	let nv_ram = unsafe { NV_RAM.as_mut_ptr().as_mut().unwrap() };
	defmt::debug!("nvram raw contents: {:?}", nv_ram);

	if !nv_ram.integrity.check() {
		defmt::warn!("NVRAM integrity check failed, initializing to defaults");
		// SAFETY: We just checked integrity above
		nv_ram.reset();
		defmt::debug!(
			"initialized nvram with integrity data {:?}",
			nv_ram.integrity
		);
	}

	nv_ram
}
