use core::{cell::UnsafeCell, mem::MaybeUninit, str::FromStr, sync::atomic::AtomicBool};

use embassy_executor::Spawner;
use embassy_sync::{
	blocking_mutex::raw::{CriticalSectionRawMutex, RawMutex},
	once_lock::OnceLock,
	semaphore::{GreedySemaphore, Semaphore},
	signal::Signal,
};

use crate::service::svc_mqtt::Mqtt;

pub static CFG_PR_RUN: Opt<bool> = Opt::new_with_default("pr/run", false);
pub static CFG_PR_TITLE: Opt<heapless::String<64>> = Opt::new("pr/title");
pub static CFG_PR_AUTHOR: Opt<heapless::String<64>> = Opt::new("pr/author");
pub static CFG_PR_NUMBER: Opt<u64> = Opt::new("pr/number");

pub static CFG_GLOBAL_POWER_TYPE: Opt<PowerType> = Opt::new("config/power_type");
pub static CFG_GLOBAL_USB_IFACE: Opt<UsbIface> = Opt::new("config/usb_iface");
pub static CFG_GLOBAL_BOOT_SOURCE: Opt<BootSource> = Opt::new("config/boot_source");
pub static CFG_GLOBAL_REQUIRE_4A_VBUS: Opt<bool> = Opt::new("config/require_4a_vbus");
pub static CFG_GLOBAL_WOL: Opt<Wol> = Opt::new("config/wol");

pub struct Config {
	pub mqtt:    &'static OnceLock<Mqtt>,
	pub spawner: Spawner,
}

#[embassy_executor::task]
pub async fn run(config: Config) {
	let Config { mqtt, spawner } = config;

	defmt::debug!("waiting for MQTT");
	let mqtt = mqtt.get().await;
	defmt::debug!("MQTT connection obtained");

	macro_rules! count_updaters {
		($_expr: expr) => ( 1 );
		($_expr: expr, $($rest:expr),+) => (
			1 + count_updaters!($($rest),+)
		);
	}

	macro_rules! spawn_cfg_updaters {
		($($stat:expr),+ $(,)?) => {
			static BARRIER: GreedySemaphore<CriticalSectionRawMutex> = GreedySemaphore::new(count_updaters!($($stat),+));

			BARRIER.try_acquire(count_updaters!($($stat),+)).unwrap().disarm();

			$({
				#[embassy_executor::task]
				async fn cfg_updater(mqtt: &'static Mqtt) -> ! {
					$stat.update(mqtt, &BARRIER).await
				}

				spawner.spawn(cfg_updater(mqtt).unwrap());
			})+

			defmt::debug!("all config loops spawned; waiting for subscriptions...");
			BARRIER.acquire(count_updaters!($($stat),+)).await.unwrap();
			defmt::debug!("all config subscriptions active");
		}
	}

	spawn_cfg_updaters!(
		CFG_PR_RUN,
		CFG_PR_TITLE,
		CFG_PR_NUMBER,
		CFG_PR_AUTHOR,
		CFG_GLOBAL_POWER_TYPE,
		CFG_GLOBAL_USB_IFACE,
		CFG_GLOBAL_BOOT_SOURCE,
		CFG_GLOBAL_REQUIRE_4A_VBUS,
		CFG_GLOBAL_WOL,
	);

	// Now tell the area controller we want our config.
	if let Err(err) = mqtt.publish_1("status/config", "ready").await {
		defmt::panic!("failed to indicate config readiness: {:?}", err);
	};
}

pub struct Opt<T>
where
	T: 'static,
{
	signal:       Signal<CriticalSectionRawMutex, T>,
	topic_suffix: &'static str,
	default:      OnceValue<Option<T>>,
}

impl<T> Opt<T>
where
	T: FromStr + 'static,
{
	const fn new(suffix: &'static str) -> Self {
		Self {
			signal:       Signal::new(),
			topic_suffix: suffix,
			default:      OnceValue::new(None),
		}
	}

	const fn new_with_default(suffix: &'static str, default: T) -> Self {
		Self {
			signal:       Signal::new(),
			topic_suffix: suffix,
			default:      OnceValue::new(Some(default)),
		}
	}

	pub const fn suffix(&self) -> &str {
		self.topic_suffix
	}

	async fn update<M: RawMutex>(&self, mqtt: &Mqtt, barrier: &'static GreedySemaphore<M>) -> ! {
		let topic = mqtt.prepare_topic(self.topic_suffix);
		let mut sub = mqtt.subscribe(&topic).await;

		// Now that we've subscribed, indicate that we are ready to receive updates.
		barrier.release(1);

		loop {
			defmt::trace!("waiting for next message on {:?}", topic);
			if let Some(v) = sub.next_message().await {
				defmt::trace!("got published message for topic {:?}", topic);
				let bytes = v.payload();
				let Ok(s) = core::str::from_utf8(bytes) else {
					defmt::warn!("got invalid UTF-8 value for config topic {:?}", topic);
					continue;
				};
				let Ok(v) = T::from_str(s) else {
					defmt::warn!("new config topic value for {:?} could not be parsed", topic);
					continue;
				};
				self.signal.signal(v);
			} else {
				defmt::trace!("got None message for {:?}; starting again", topic);
			}
		}
	}

	pub async fn next(&self) -> T {
		if let Some(Some(v)) = self.default.take() {
			return v;
		}

		self.signal.wait().await
	}
}

struct OnceValue<T>
where
	T: 'static,
{
	taken: AtomicBool,
	cell:  UnsafeCell<MaybeUninit<T>>,
}

impl<T> OnceValue<T>
where
	T: 'static,
{
	pub const fn new(v: T) -> Self {
		Self {
			taken: AtomicBool::new(false),
			cell:  UnsafeCell::new(MaybeUninit::new(v)),
		}
	}

	pub fn take(&self) -> Option<T> {
		if self.taken.swap(true, core::sync::atomic::Ordering::SeqCst) {
			// Already taken
			None
		} else {
			Some(unsafe {
				core::ptr::replace(self.cell.get(), MaybeUninit::uninit()).assume_init()
			})
		}
	}
}

unsafe impl<T: Send> Send for OnceValue<T> {}
unsafe impl<T: Send> Sync for OnceValue<T> {}

#[derive(Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum PowerType {
	Usb,
	UsbVbus,
	Psu,
}

impl FromStr for PowerType {
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"usb" => Ok(Self::Usb),
			"vbus" => Ok(Self::UsbVbus),
			"psu" => Ok(Self::Psu),
			other => {
				defmt::warn!("got invalid power type: {}", other);
				Err(())
			}
		}
	}
}

#[derive(Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum UsbIface {
	Port,
	Header,
}

impl FromStr for UsbIface {
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"port" => Ok(Self::Port),
			"header" => Ok(Self::Header),
			other => {
				defmt::warn!("got invalid usb iface type: {}", other);
				Err(())
			}
		}
	}
}

#[derive(Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum BootSource {
	UsbMsd,
	Sd,
}

impl FromStr for BootSource {
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"usb_msd" => Ok(Self::UsbMsd),
			"sd" => Ok(Self::Sd),
			other => {
				defmt::warn!("got invalid boot source type: {}", other);
				Err(())
			}
		}
	}
}

#[derive(Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum Wol {
	Off,
	Mins5,
	Mins10,
	Mins30,
}

impl FromStr for Wol {
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"off" => Ok(Self::Off),
			"5m" => Ok(Self::Mins5),
			"10m" => Ok(Self::Mins10),
			"30m" => Ok(Self::Mins30),
			other => {
				defmt::warn!("got invalid WoL setting: {}", other);
				Err(())
			}
		}
	}
}
