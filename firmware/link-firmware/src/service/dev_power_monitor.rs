use embassy_stm32::{
	i2c::{I2c, mode::Master},
	mode::Blocking,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};

use crate::service::svc_mqtt_stats::{QoS, Stat};

pub static STAT_CURRENT: Stat<heapless::String<16>, { QoS::Q0 }> = Stat::new("power/current_mA");

const ADDR: u8 = 0x40;
const MA_CALIBRATION: u16 = 0x0A00;
// NOTE: this is not the maximum rating (which is 2A) but really
//       the maximum reportable value. We keep this a bit higher
//       such that we can see if values go higher than 2A.
const BOARD_MAX_RATED_MA: u16 = 4000;

pub struct Config {
	pub i2c: &'static Mutex<NoopRawMutex, I2c<'static, Blocking, Master>>,
}

#[embassy_executor::task]
pub async fn run(config: Config) -> ! {
	let Config { i2c } = config;

	macro_rules! set {
		($reg:expr,[$high:expr, $low:expr]) => {{
			let mut i2c = i2c.lock().await;
			if let Err(err) = i2c.blocking_write(ADDR, &[$reg, $high, $low]) {
				defmt::error!("failed to write to power monitor chip: {:?}", err);
			}
		}};
		($reg:expr, $value:expr) => {{
			let val = u16::from($value);
			set!($reg, [(val >> 8) as u8, val as u8]);
		}};
	}

	macro_rules! get {
		($reg:expr) => {{
			let mut i2c = i2c.lock().await;
			let mut buf = [0; 2];
			if let Err(err) = i2c.blocking_write_read(ADDR, &[$reg], &mut buf) {
				defmt::error!("failed to read from power monitor chip: {:?}", err);
			}
			u16::from_be_bytes(buf)
		}};
	}

	// Reset
	defmt::info!("resetting power monitor chip...");
	set!(0x00, Configuration::reset());
	Timer::after(Duration::from_millis(10)).await;
	if get!(0x00) == Configuration::default().0 {
		defmt::info!("power monitor chip reset successful");
	} else {
		defmt::error!("power monitor chip reset failed");
	}

	// Print the manu ID.
	let manuid = get!(0xFE);
	defmt::info!("power monitor chip manufacturer ID: {:04X}", manuid);
	let dieid = get!(0xFF);
	defmt::info!("power monitor chip die ID: {:04X}", dieid);
	Timer::after(Duration::from_millis(10)).await;

	// Calculate the calibration value.
	let calibration = calculate_high_res_calibration(BOARD_MAX_RATED_MA);
	defmt::debug!("power monitor calibration value: {} (dec)", calibration);

	// Set the configuration value
	set!(
		0x00,
		Configuration::new()
			.with_average_samples(AverageSamples::Avg64)
			.with_bus_conversion_time(ConverstionTime::Us140)
			.with_shunt_conversion_time(ConverstionTime::Us140)
			.with_mode(Mode::ShuntAndBusContinuous)
	);

	// Set the alert pin mode (mask/enable register)
	set!(
		0x06,
		MaskEnableRegister::new()
			.with_power_over_limit()
			.with_alert_latch()
	);

	// Set the current (in mA) that will trigger the OC alert pin
	defmt::debug!(
		"power monitor chip will alert on OC after {}mA",
		super::failsafe_board_oc::ALERT_ON_CURRENT_MA
	);
	let power_alert_value = calculate_alert_power_value(
		// 5V = 5000 millivolts
		5000,
		super::failsafe_board_oc::ALERT_ON_CURRENT_MA,
		calibration,
	);
	defmt::debug!("power alert level value: {} (dec)", power_alert_value);
	set!(0x07, power_alert_value);

	defmt::info!("configured power monitor chip");
	Timer::after(Duration::from_millis(10)).await;

	// Set the calibration register. The board uses a 2mOhm shunt resistor.
	set!(0x05, calibration);
	defmt::info!("calibrated power monitor chip");

	loop {
		Timer::after(Duration::from_millis(250)).await;
		let raw_current = get!(0x04);
		let (current_ma, current_ua) = calculate_ma_from_calibration(raw_current, calibration);
		defmt::trace!(
			"powermon: current: {} ({}.{}mA)",
			raw_current,
			current_ma,
			current_ua
		);

		STAT_CURRENT.set(heapless::format!("{current_ma}.{current_ua}").unwrap());
	}
}

#[derive(Clone, Copy)]
#[repr(transparent)]
struct Configuration(u16);

impl Configuration {
	fn new() -> Self {
		Self(0x0000)
	}

	fn reset() -> Self {
		Self(0x8000)
	}

	fn with_average_samples(self, samples: AverageSamples) -> Self {
		Self((self.0 & !(0b111 << 9)) | ((samples as u16) << 9))
	}

	fn with_bus_conversion_time(self, time: ConverstionTime) -> Self {
		Self((self.0 & !(0b111 << 6)) | ((time as u16) << 6))
	}

	fn with_shunt_conversion_time(self, time: ConverstionTime) -> Self {
		Self((self.0 & !(0b111 << 3)) | ((time as u16) << 3))
	}

	fn with_mode(self, mode: Mode) -> Self {
		Self((self.0 & !0b111) | (mode as u16))
	}
}

impl Default for Configuration {
	#[inline]
	fn default() -> Self {
		Self(0x4127)
	}
}

impl From<u16> for Configuration {
	#[inline]
	fn from(value: u16) -> Self {
		Self(value)
	}
}

impl From<Configuration> for u16 {
	#[inline]
	fn from(value: Configuration) -> Self {
		value.0
	}
}

#[derive(Clone, Copy)]
#[repr(transparent)]
struct MaskEnableRegister(u16);

#[expect(unused, reason = "here more for completeness, most won't be used")]
impl MaskEnableRegister {
	fn new() -> Self {
		Self(0)
	}

	fn with_shunt_overvoltage(self) -> Self {
		Self(self.0 | (1 << 15))
	}

	fn with_shunt_undervoltage(self) -> Self {
		Self(self.0 | (1 << 14))
	}

	fn with_bus_overvoltage(self) -> Self {
		Self(self.0 | (1 << 13))
	}

	fn with_bus_undervoltage(self) -> Self {
		Self(self.0 | (1 << 12))
	}

	fn with_power_over_limit(self) -> Self {
		Self(self.0 | (1 << 11))
	}

	fn with_conversion_ready(self) -> Self {
		Self(self.0 | (1 << 10))
	}

	fn with_alert_polarity_invert(self) -> Self {
		Self(self.0 | (1 << 1))
	}

	fn with_alert_latch(self) -> Self {
		Self(self.0 | (1 << 0))
	}
}

impl Default for MaskEnableRegister {
	#[inline]
	fn default() -> Self {
		Self::new()
	}
}

impl From<u16> for MaskEnableRegister {
	#[inline]
	fn from(v: u16) -> Self {
		Self(v)
	}
}

impl From<MaskEnableRegister> for u16 {
	#[inline]
	fn from(v: MaskEnableRegister) -> Self {
		v.0
	}
}

#[derive(Clone, Copy)]
#[repr(u16)]
#[allow(dead_code)]
enum AverageSamples {
	Avg1    = 0b000,
	Avg4    = 0b001,
	Avg16   = 0b010,
	Avg64   = 0b011,
	Avg128  = 0b100,
	Avg256  = 0b101,
	Avg512  = 0b110,
	Avg1024 = 0b111,
}

#[derive(Clone, Copy)]
#[repr(u16)]
#[allow(dead_code)]
enum ConverstionTime {
	Us140   = 0b000,
	Us204   = 0b001,
	Us332   = 0b010,
	Us588   = 0b011,
	Ms1p1   = 0b100,
	Ms2p116 = 0b101,
	Ms4p156 = 0b110,
	Ms8p244 = 0b111,
}

#[derive(Clone, Copy)]
#[repr(u16)]
#[allow(dead_code)]
enum Mode {
	PowerDown              = 0b000,
	ShuntVoltageTriggered  = 0b001,
	BusVoltageTriggered    = 0b010,
	ShuntAndBusTriggered   = 0b011,
	#[deprecated(note = "duplicate value; use PowerDown instead")]
	PowerDown2             = 0b100,
	ShuntVoltageContinuous = 0b101,
	BusVoltageContinuous   = 0b110,
	ShuntAndBusContinuous  = 0b111,
}

/// Calculates the alert power value based
/// on the expected bus voltage and the current
/// limit in milliamps, along with the calibration
/// register.
///
/// The power register is used to compare to the
/// limit value in order to trigger the alert.
///
/// The power register is calculated as such:
///
///    BusVoltage * Current
///    --------------------
///           20_000
///
/// This chip treats the bus voltage register
/// as 1.25mV per bit. Thus, to translate actual
/// voltage to the chip's voltage value:
///
///            mV
///          ------   OR   mV * 0.8
///           1.25
///
/// To avoid FP issues on embedded, we multiply by
/// 8 and then divide by 10.
///
/// For example, 5V = 5000mV * 0.8 = 4000(dec)
///
/// The calibration register is used to set the
/// resolution of the current register.
///
///   Current = ShuntVoltage * Calibration
///             --------------------------
///                        2048
fn calculate_alert_power_value(millivolts: u16, milliamp_limit: u16, calibration: u16) -> u16 {
	// First find the stable bus voltage value.
	let bus_voltage = (u32::from(millivolts) * 8) / 10;
	// First find the shunt voltage needed if the calibration
	// is at a perfect mA level (0x0A00).
	//
	// (Current * 2048) / Calibration
	let shunt_at_ma = (u32::from(milliamp_limit) * 2048) / u32::from(MA_CALIBRATION);
	// Re-calculate the reported current at the limit.
	let current_limit = (shunt_at_ma * u32::from(calibration)) / 2048;
	// Multiply by the bus voltage to arrive at the
	// power register level that will trigger the
	// alert.
	((bus_voltage * current_limit) / 20_000) as u16
}

/// Calculates the calibration register for receiving the
/// a high resolution given the maximum rated current
/// for the board.
///
/// See [`calculate_alert_power_value()`] and the datasheet
/// for how these formulas work.
fn calculate_high_res_calibration(max_rated_ma: u16) -> u16 {
	// First find the shunt value at the maximum mA value
	// given a standard calibration level for perfect
	// mA values (0x0A00).
	let shunt_at_ma = (u32::from(max_rated_ma) * 2048) / u32::from(MA_CALIBRATION);
	// Re-adjust the calibration such that the maximum
	// value is the "highest" current value.
	//
	// NOTE: the calibration value is a SIGNED i16,
	// but we only ever want positive values, so we
	// will use a maximum value of i16::MAX.
	const MAX_VALUE: u16 = i16::MAX as u16;
	((u32::from(MAX_VALUE) * 2048) / shunt_at_ma) as u16
}

/// Converts the calibrated current value to milliamps.
///
/// See [`calculate_alert_power_value()`] and the datasheet
/// for how these formulas work.
fn calculate_ma_from_calibration(current_register: u16, calibration: u16) -> (u16, u16) {
	// First, get the bus shunt value for the
	// given calibration value.
	let shunt_value = (u32::from(current_register) * 2048) / u32::from(calibration);
	// Now find the current value. We use the standard mA value (0x0A00) but
	// multiply it by 1000 in order to get the microamp values.
	let current_ua = (shunt_value * (u32::from(MA_CALIBRATION) * 1000)) / 2048;

	((current_ua / 1000) as u16, (current_ua % 1000) as u16)
}
