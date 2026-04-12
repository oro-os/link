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
	integrity:   Integrity,
	pub reboot:  NvRamRebootStats,
	pub failure: Volatile<LastBootFailure>,
}

impl NvRam {
	pub fn reset(&mut self) {
		self.integrity.reset();
		self.reboot.reset();
		self.failure.take_and_reset();
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

#[derive(defmt::Format, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum LastBootFailure {
	/// There was no failure; board rebooted
	/// gracefully.
	#[default]
	None           = 0,
	/// The board's power monitor (board-wide 5V monitor)
	/// tripped the OC interrupt.
	PowerMonitorOC = 1,
	/// The aux VBUS OC warn line was asserted.
	AuxVbusOC      = 2,
	/// The main VBUS OC warn line was asserted.
	VbusOC         = 3,
	/// The ULPI VBUS OC line was asserted.
	UlpiOC         = 4,
}

impl LastBootFailure {
	pub const fn as_str(&self) -> &'static str {
		match self {
			Self::None => "ok",
			Self::PowerMonitorOC => "power monitor (board 5V) overcurrent",
			Self::AuxVbusOC => "high-current VBUS line overcurrent",
			Self::VbusOC => "main (low-current) VBUS line overcurrent",
			Self::UlpiOC => "ULPI VBUS line overcurrent",
		}
	}
}

impl AsRef<[u8]> for LastBootFailure {
	fn as_ref(&self) -> &[u8] {
		self.as_str().as_bytes()
	}
}

pub trait VolatileLastBootFailure {
	fn take_and_reset(&mut self) -> LastBootFailure;
}

impl VolatileLastBootFailure for Volatile<LastBootFailure> {
	fn take_and_reset(&mut self) -> LastBootFailure {
		let v = self.read();
		self.write(LastBootFailure::default());
		v
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
