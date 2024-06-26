#[cfg(feature = "stm32f479vg")]
mod stm32f479vg;
#[cfg(feature = "stm32f479vg")]
pub use stm32f479vg::*;

use crate::chip;
use cortex_m::peripheral::SCB;
use embassy_stm32::{
	gpio::{Input, Output},
	i2c,
	mode::Mode,
	usart,
};
use embassy_time::{block_for, Duration, Instant};

use defmt_rtt as _;

defmt::timestamp!("{=u64:us}", Instant::now().as_micros());

/// Implementation of I2c proxies for STM32 I2c peripherals.
impl<'d, M: Mode> chip::I2c for i2c::I2c<'d, M> {
	type Error = i2c::Error;

	#[inline]
	fn read(&mut self, addr: u8, buffer: &mut [u8]) -> Result<(), Self::Error> {
		i2c::I2c::blocking_read(self, addr, buffer)
	}

	#[inline]
	fn write(&mut self, addr: u8, buffer: &[u8]) -> Result<(), Self::Error> {
		i2c::I2c::blocking_write(self, addr, buffer)
	}
}

/// Implements a (`DebugLed`)(super::DebugLed) for a single STM32 pin.
pub struct DebugLed<'d> {
	pin: Output<'d>,
}

impl<'d> DebugLed<'d> {
	pub fn new(pin: Output<'d>) -> Self {
		Self { pin }
	}
}

impl super::DebugLed for DebugLed<'_> {
	fn set_bit(&mut self, on: bool) {
		self.pin.set_level(on.into());
	}
}

/// Implements a (`SystemUnderTest`)[super::SystemUnderTest] for a collection of STM32 pins.
pub struct SystemUnderTest<'d> {
	current_state: super::PowerState,
	reset_pin: Output<'d>,
	power_pin: Output<'d>,
	psu_on_pin: Output<'d>,
	psu_standby_pin: Output<'d>,
	sys_on_pin: Input<'d>,
}

impl<'d> SystemUnderTest<'d> {
	pub fn new(
		reset_pin: Output<'d>,
		power_pin: Output<'d>,
		psu_on_pin: Output<'d>,
		psu_standby_pin: Output<'d>,
		sys_on_pin: Input<'d>,
	) -> Self {
		Self {
			current_state: super::PowerState::Off,
			reset_pin,
			power_pin,
			psu_on_pin,
			psu_standby_pin,
			sys_on_pin,
		}
	}
}

impl<'d> super::SystemUnderTest for SystemUnderTest<'d> {
	fn reset_ms(&mut self, ms: u64) {
		self.reset_pin.set_high();
		block_for(Duration::from_millis(ms));
		self.reset_pin.set_low();
	}

	fn power_ms(&mut self, ms: u64) {
		self.power_pin.set_high();
		block_for(Duration::from_millis(ms));
		self.power_pin.set_low();
	}

	fn current_state(&self) -> super::PowerState {
		self.current_state
	}

	fn power_requested(&self) -> bool {
		self.sys_on_pin.is_low()
	}

	unsafe fn set_power_state(&mut self, new_state: super::PowerState) {
		match new_state {
			super::PowerState::Off => {
				self.psu_on_pin.set_low();
				self.psu_standby_pin.set_low();
			}
			super::PowerState::Standby => {
				self.psu_on_pin.set_low();
				self.psu_standby_pin.set_high();
			}
			super::PowerState::On => {
				self.psu_on_pin.set_high();
				self.psu_standby_pin.set_high();
			}
		}

		self.current_state = new_state;
	}
}

impl From<super::DateTime> for embassy_stm32::rtc::DateTime {
	fn from(v: super::DateTime) -> Self {
		use embassy_stm32::rtc::DayOfWeek as D;
		Self::from(
			v.year,
			v.month,
			v.day,
			match v.day_of_week {
				0 => D::Sunday,
				1 => D::Monday,
				2 => D::Tuesday,
				3 => D::Wednesday,
				4 => D::Thursday,
				5 => D::Friday,
				6 => D::Saturday,
				_ => panic!(),
			},
			v.hour,
			v.minute,
			v.second,
		)
		.unwrap()
	}
}

impl super::WallClock for embassy_stm32::rtc::Rtc {
	fn set_datetime(&mut self, dt: super::DateTime) {
		embassy_stm32::rtc::Rtc::set_daylight_savings(self, dt.dst);
		embassy_stm32::rtc::Rtc::set_datetime(self, dt.into()).ok();
	}

	fn get_datetime(&self) -> Option<super::DateTime> {
		self.now()
			.map(|dt| {
				use embassy_stm32::rtc::DayOfWeek as D;
				super::DateTime {
					year: dt.year(),
					month: dt.month(),
					day: dt.day(),
					day_of_week: match dt.day_of_week() {
						D::Sunday => 0,
						D::Monday => 1,
						D::Tuesday => 2,
						D::Wednesday => 3,
						D::Thursday => 4,
						D::Friday => 5,
						D::Saturday => 6,
					},
					hour: dt.hour(),
					minute: dt.minute(),
					second: dt.second(),
					dst: self.get_daylight_savings(),
				}
			})
			.ok()
	}
}

pub struct StmUniqueId;

impl super::UniqueId for StmUniqueId {
	fn unique_id(&self) -> [u8; 32] {
		use sha2::Digest;

		let mut sha256 = sha2::Sha256::new();

		for i in 0..3 {
			sha256.update(stm32_metapac::UID.uid(i).read().to_be_bytes());
		}

		sha256.finalize().into()
	}
}

pub fn get_exteth_mac() -> [u8; 6] {
	let hash = <StmUniqueId as super::UniqueId>::unique_id(&StmUniqueId);

	let mut macaddr = [0u8; 6];
	macaddr[0] = b'.';
	macaddr[1] = b'o';
	macaddr[2] = b'O';
	macaddr[3] = hash[29];
	macaddr[4] = hash[30];
	macaddr[5] = hash[31];

	macaddr
}

impl<'d, M: Mode> super::PacketTracer for usart::UartTx<'d, M> {
	fn trace_packet(&mut self, buf: &[u8]) {
		debug_assert!(buf.len() <= u16::MAX as usize);
		let len_bytes = (buf.len() as u16).to_be_bytes();
		self.blocking_write(&len_bytes[..]).unwrap();
		self.blocking_write(buf).unwrap();
		self.blocking_flush().unwrap();
	}
}

pub struct CortexResetManager;

impl super::ResetManager for CortexResetManager {
	fn reset(self) -> ! {
		SCB::sys_reset()
	}
}
