use core::{fmt::Display, str::FromStr, sync::atomic::AtomicBool};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, signal::Signal};
use embassy_time::{Duration, Timer};
use heapless::String;

use crate::atomic::Relaxed;

pub static DIRTY_FLAG: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[macro_export]
macro_rules! vars {
	($($perm:ident : $T:ty => $id:ident($name:literal) $(= $def:expr)?),* $(,)?) => {
		$(
			pub static $id: Key<$T, {Perm::$perm.read()}, {Perm::$perm.write()}> = Key::new($name)$(.with_default($def))?;
		)*

		macro_rules! foreach_var {
			($ident:ident => $blk:block) => (
				$({
					let $ident = &$crate::vars::$id;
					$blk
				})*
			)
		}

		pub(crate) use foreach_var;
	}
}

vars! {
	RO:i64 => STAT_BOARD_CURRENT_MA("board_current_ma"),
	RO:i64 => STAT_BOARD_CURRENT_UA("board_current_ua"),
	RO:String<8> => STAT_VBUS_POWER_STATE("vbus_power_state"),
	RO:String<4> => STAT_OLED_POWER_STATE("oled_power_state"),
	RO:String<4> => STAT_OLED_POWER_TARGET_STATE("oled_power_target_state"),
	RO:i64 => STAT_BOARD_OC_MA("board_failsafe_oc_ma"),
	RO:String<16> => STAT_BLINKEN_LIGHT_CMD("blinken_light_cmd"),
	RO:bool => STAT_OLED_POWER_VREG("oled_power_vreg"),
	RO:i64 => STAT_OLED_BRIGHTNESS("oled_brightness"),
	RO:bool => STAT_LEDS_CHIP_ENABLED("leds_chip_enabled"),
	RO:String<16> => STAT_LEDS_STATE("leds_state"),
	RO:String<16> => STAT_LEDS_TARGET_STATE("leds_target_state"),
	RO:bool => STAT_PSU_ON("psu_on"),
	RO:bool => STAT_INITIALIZED("initialized"),
	RO:i64 => STAT_VERSION_MAJOR("version_major"),
	RO:i64 => STAT_VERSION_MINOR("version_minor"),
	RO:i64 => STAT_VERSION_PATCH("version_patch"),
	RO:String<128> => STAT_LAST_BOOT_FAILURE("last_boot_failure"),
	RO:bool => STAT_AUX_VBUS_SENSE("aux_vbus_sense"),
	RW:bool => CFG_PR_RUN("pr_run"),
	WO:String<128> => CFG_PR_TITLE("pr_title"),
	WO:String<128> => CFG_PR_AUTHOR("pr_author"),
	WO:i64 => CFG_PR_NUMBER("pr_number"),
	WO:PowerType => CFG_SUT_POWER_TYPE("sut_power_type") = PowerType::Psu,
	WO:bool => CFG_CONFIGURED("configured"),
	WO:u32 => CFG_WOL("wol"),
	WO:UsbIface => CFG_SUT_USB_IFACE("sut_usb_iface") = UsbIface::Header,
	WO:BootSource => CFG_SUT_BOOT_SOURCE("sut_boot_source") = BootSource::UsbMsd,
	WO:bool => CFG_SUT_REQUIRE_4A_VBUS("sut_require_4a_vbus"),
}

macro_rules! var_enum {
	($(#[$attr:meta])* $vis:vis enum $name:ident { $($(#[$variant_attr:meta])* $variant:ident = $variant_str:literal),* $(,)? }) => {
		$(#[$attr])*
		$vis enum $name {
			$($(#[$variant_attr])* $variant),*
		}

		impl FromStr for $name {
			type Err = ();

			fn from_str(s: &str) -> Result<Self, Self::Err> {
				match s {
					$($variant_str => Ok(Self::$variant),)*
					_ => Err(()),
				}
			}
		}

		impl core::fmt::Display for $name {
			fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
				match self {
					$(Self::$variant => write!(f, "{}", $variant_str),)*
				}
			}
		}
	};
}

var_enum! {
	#[derive(Clone, Copy, PartialEq, Eq, defmt::Format)]
	pub enum PowerType {
		Usb = "usb",
		UsbVbus = "usb_vbus",
		Psu = "psu",
	}
}

var_enum! {
	#[derive(Clone, Copy, PartialEq, Eq, defmt::Format)]
	pub enum UsbIface {
		Port = "port",
		Header = "header",
	}
}

var_enum! {
	#[derive(Clone, Copy, PartialEq, Eq, defmt::Format)]
	pub enum BootSource {
		UsbMsd = "usb",
		Sd = "sd",
	}
}

pub struct Key<T, const READ: bool, const WRITE: bool> {
	name:  &'static str,
	value: Mutex<CriticalSectionRawMutex, Option<T>>,
	dirty: AtomicBool,
}

impl<T, const READ: bool, const WRITE: bool> Key<T, READ, WRITE> {
	const fn new(name: &'static str) -> Self {
		Self {
			name,
			value: Mutex::new(None),
			dirty: AtomicBool::new(READ),
		}
	}

	const fn with_default(mut self, default: T) -> Self {
		core::mem::forget(core::mem::replace(
			&mut self.value,
			Mutex::new(Some(default)),
		));
		self.dirty = AtomicBool::new(true);
		self
	}

	pub async fn set(&self, value: T)
	where
		Self: CanRead,
		T: Display,
	{
		let mut lock = self.value.lock().await;
		*lock = Some(value);
		self.dirty.set(true);
		DIRTY_FLAG.signal(());
	}

	#[expect(unused)]
	pub async fn unset(&self)
	where
		Self: CanRead,
	{
		let mut lock = self.value.lock().await;
		*lock = None;
		self.dirty.set(true);
		DIRTY_FLAG.signal(());
	}

	pub async fn get(&self) -> T
	where
		Self: CanWrite,
		T: Clone,
	{
		loop {
			if let Some(lock) = self.value.lock().await.clone() {
				return lock;
			};
			Timer::after(Duration::from_millis(50)).await;
		}
	}

	pub async fn wait_for(&self, desired: Option<&T>)
	where
		Self: CanWrite,
		T: PartialEq,
	{
		loop {
			let lock = self.value.lock().await;
			if lock.as_ref() == desired {
				break;
			}
			drop(lock);
			Timer::after(Duration::from_millis(50)).await;
		}
	}
}

pub trait CanRead {}
pub trait CanWrite {}

impl<T, const WRITE: bool> CanRead for Key<T, true, WRITE> {}
impl<T, const READ: bool> CanWrite for Key<T, READ, true> {}

#[derive(Copy, Clone)]
enum Perm {
	RO,
	RW,
	WO,
}

impl Perm {
	const fn read(self) -> bool {
		matches!(self, Self::RO | Self::RW)
	}

	const fn write(self) -> bool {
		matches!(self, Self::WO | Self::RW)
	}
}

impl<T: Display> SyncVar for Key<T, true, false> {
	async fn sync<const N: usize>(
		&self,
		redis: &mut crate::redis::Client<'_, N>,
	) -> crate::redis::Result<()> {
		if self.dirty.set(false) {
			let value = self.value.lock().await;
			if let Some(value) = value.as_ref() {
				redis.set(self.name, value).await?;
			} else {
				redis.del(self.name).await?;
			}
		}

		Ok(())
	}
}

impl<T: Display + FromStr> SyncVar for Key<T, true, true> {
	async fn sync<const N: usize>(
		&self,
		redis: &mut crate::redis::Client<'_, N>,
	) -> crate::redis::Result<()> {
		if self.dirty.set(false) {
			let value = self.value.lock().await;
			if let Some(value) = value.as_ref() {
				redis.set(self.name, value).await?;
			} else {
				redis.del(self.name).await?;
			}
		} else {
			let mut lock = self.value.lock().await;
			let remote = redis.get(self.name).await?;
			*lock = remote;
		}

		Ok(())
	}
}

impl<T: Display + FromStr> SyncVar for Key<T, false, true> {
	async fn sync<const N: usize>(
		&self,
		redis: &mut crate::redis::Client<'_, N>,
	) -> crate::redis::Result<()> {
		let mut lock = self.value.lock().await;
		let remote = redis.get(self.name).await?;
		*lock = remote;
		Ok(())
	}
}

pub trait SyncVar {
	async fn sync<const N: usize>(
		&self,
		redis: &mut crate::redis::Client<'_, N>,
	) -> crate::redis::Result<()>;
}
