use embassy_executor::Spawner;
use embassy_sync::{
	blocking_mutex::raw::CriticalSectionRawMutex, once_lock::OnceLock, signal::Signal,
};

use crate::service::svc_mqtt::{Mqtt, PrefixedTopic};

/// Which QoS to use when publishing a stat
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, core::marker::ConstParamTy)]
#[repr(u8)]
pub enum QoS {
	Q0 = 0,
	Q1 = 1,
	// NOTE: This is not supported by rmqtt, so we don't support it here.
	// Q2 = 2,
}

/// Implements a firmware-wide global stat; must be listed
/// in the `svc_mqtt_stats` service to be transmitted.
pub struct Stat<T, const QOS: QoS = { QoS::Q1 }, const RETAIN: bool = true> {
	signal:       Signal<CriticalSectionRawMutex, T>,
	topic_suffix: &'static str,
	topic:        OnceLock<PrefixedTopic>,
}

/// A stat where the contents are stringized; if possible, use a
/// [`Stat`] with an implementation of `AsRef<[u8]>` on the type
/// instead.
pub type StrStat<T, const SZ: usize = 16, const QOS: QoS = { QoS::Q1 }, const RETAIN: bool = true> =
	Stat<Stringized<T, SZ>, QOS, RETAIN>;

/// A boolean stat.
pub type BoolStat<const QOS: QoS = { QoS::Q1 }, const RETAIN: bool = true> =
	Stat<Boolized, QOS, RETAIN>;

impl<T, const QOS: QoS, const RETAIN: bool> Stat<T, QOS, RETAIN> {
	pub const fn new(topic: &'static str) -> Self {
		Self {
			signal:       Signal::new(),
			topic_suffix: topic,
			topic:        OnceLock::new(),
		}
	}

	pub fn set(&self, v: impl Into<T>) {
		self.signal.signal(v.into());
	}

	pub fn try_set<V: TryInto<T>>(&self, v: V) -> Result<(), V::Error> {
		self.signal.signal(v.try_into()?);
		Ok(())
	}
}

trait UpdateStat {
	async fn update(&self, mqtt: &Mqtt);
}

macro_rules! impl_update {
	($($method:ident($q:ident, $retain:expr, $fail_message:literal)),* $(,)?) => {
		$(impl<T: AsRef<[u8]>> UpdateStat for Stat<T, { QoS::$q }, $retain> {
			async fn update(&self, mqtt: &Mqtt) {
				let topic = self.topic.get_or_init(|| mqtt.prepare_topic(self.topic_suffix));
				let v = self.signal.wait().await;
				defmt::trace!("updating topic {:?}", topic);
				if let Err(err) = mqtt.$method(topic, v).await {
					defmt::warn!($fail_message, topic, err);
				}
				defmt::trace!("updated topic {:?}", topic);
			}
		})*
	}
}

impl_update! {
	publish_0(Q0, false, "failed to publish topic (QoS=0, no-retain): {:?}: {:?}"),
	publish_1(Q1, false, "failed to publish topic (QoS=1, no-retain): {:?}: {:?}"),
//	publish_2(Q2, false, "failed to publish topic (QoS=2, no-retain): {:?}: {:?}"),
	retain_0(Q0, true, "failed to publish topic (QoS=0, retain): {:?}: {:?}"),
	retain_1(Q1, true, "failed to publish topic (QoS=1, retain): {:?}: {:?}"),
//	retain_2(Q2, true, "failed to publish topic (QoS=2, retain): {:?}: {:?}"),
}

pub struct Config {
	pub mqtt:    &'static OnceLock<Mqtt>,
	pub spawner: Spawner,
}

#[embassy_executor::task]
pub async fn run(config: Config) {
	let Config { mqtt, spawner } = config;

	let mqtt = mqtt.get().await;

	macro_rules! spawn_stat_updaters {
		($($stat:expr),* $(,)?) => {$({
			#[embassy_executor::task]
			async fn stat_updater(mqtt: &'static Mqtt) -> ! {
				loop {
					$stat.update(mqtt).await;
				}
			}

			spawner.spawn(stat_updater(mqtt).unwrap());
		})*}
	}

	spawn_stat_updaters!(
		super::svc_oled_pwr::STAT_PWR_STATE,
		super::svc_oled_pwr::STAT_PWR_TARGET,
		super::dev_power_monitor::STAT_CURRENT,
		super::failsafe_board_oc::STAT_OC_MA,
		super::dev_blinken_light::STAT_CMD,
		super::dev_oled::STAT_PWR_VREG,
		super::dev_oled::STAT_BRIGHTNESS,
		super::dev_leds::STAT_CHIP_ENABLED,
		super::svc_leds::STAT_STATE,
		super::svc_leds::STAT_TARGET,
		crate::STAT_INITIALIZED,
		crate::STAT_VERSION_MAJOR,
		crate::STAT_VERSION_MINOR,
		crate::STAT_VERSION_PATCH,
	);

	defmt::debug!("all stat spawners have started; finishing run");
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Boolized(bool);

impl From<bool> for Boolized {
	fn from(value: bool) -> Self {
		Self(value)
	}
}

impl AsRef<[u8]> for Boolized {
	fn as_ref(&self) -> &[u8] {
		match self.0 {
			true => "true".as_ref(),
			false => "false".as_ref(),
		}
	}
}

pub struct Stringized<T, const SZ: usize = 16>(heapless::String<SZ>, core::marker::PhantomData<T>);

impl<T, const SZ: usize> From<T> for Stringized<T, SZ>
where
	T: core::fmt::Display,
{
	fn from(value: T) -> Self {
		Self(
			match heapless::format!("{value}") {
				Ok(v) => v,
				Err(_) => {
					defmt::warn!("failed to convert to stringized stat: too long");
					heapless::String::new()
				}
			},
			core::marker::PhantomData,
		)
	}
}

impl<T, const SZ: usize> AsRef<[u8]> for Stringized<T, SZ> {
	fn as_ref(&self) -> &[u8] {
		self.0.as_ref()
	}
}
