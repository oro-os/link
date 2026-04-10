use core::{cell::UnsafeCell, mem::MaybeUninit, str::FromStr, sync::atomic::AtomicBool};

use embassy_executor::Spawner;
use embassy_sync::{
	blocking_mutex::raw::CriticalSectionRawMutex, once_lock::OnceLock, signal::Signal,
};

use crate::service::svc_mqtt::Mqtt;

pub static CFG_PR_RUN: Opt<bool> = Opt::new_with_default("pr/run", false);
pub static CFG_PR_TITLE: Opt<heapless::String<64>> = Opt::new("pr/title");
pub static CFG_PR_AUTHOR: Opt<heapless::String<64>> = Opt::new("pr/author");
pub static CFG_PR_NUMBER: Opt<u64> = Opt::new("pr/number");

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

	macro_rules! spawn_cfg_updaters {
		($($stat:expr),* $(,)?) => {$({
			#[embassy_executor::task]
			async fn cfg_updater(mqtt: &'static Mqtt) -> ! {
				$stat.update(mqtt).await
			}

			spawner.spawn(cfg_updater(mqtt).unwrap());
		})*}
	}

	spawn_cfg_updaters!(CFG_PR_RUN, CFG_PR_TITLE, CFG_PR_NUMBER, CFG_PR_AUTHOR,);

	defmt::debug!("all config loops spawned");
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

	async fn update(&self, mqtt: &Mqtt) -> ! {
		let topic = mqtt.prepare_topic(self.topic_suffix);
		let mut sub = mqtt.subscribe(&topic).await;

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
