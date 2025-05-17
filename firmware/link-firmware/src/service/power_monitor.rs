use defmt::{error, info};
use embassy_stm32::{i2c::I2c, mode::Async};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, channel::Channel, mutex::Mutex};
use embassy_time::{Duration, Timer};

const ADDR: u8 = 0x40;

#[embassy_executor::task]
pub async fn power_monitor(i2c: &'static Mutex<NoopRawMutex, I2c<'static, Async>>) -> ! {
	macro_rules! set {
		($reg: expr, [ $high:expr, $low:expr ]) => {{
			let mut i2c = i2c.lock().await;
			if let Err(err) = i2c.blocking_write(ADDR, &[$reg, $high, $low]) {
				error!("failed to write to power monitor chip: {:?}", err);
			}
		}};
		($reg: expr, $value:expr) => {{
			let val = u16::from($value);
			set!($reg, [(val >> 8) as u8, val as u8]);
		}};
	}

	macro_rules! get {
		($reg: expr) => {{
			let mut i2c = i2c.lock().await;
			info!("got lock");
			let mut buf = [0; 2];
			if let Err(err) = i2c.blocking_write_read(ADDR, &[$reg], &mut buf) {
				error!("failed to read from power monitor chip: {:?}", err);
			}
			u16::from_be_bytes(buf)
		}};
	}

	// Reset
	info!("resetting power monitor chip...");
	set!(0x00, Configuration::reset());
	Timer::after(Duration::from_millis(10)).await;
	if get!(0x00) == Configuration::default().0 {
		info!("power monitor chip reset successful");
	} else {
		error!("power monitor chip reset failed");
	}

	// Print the manu ID.
	let manuid = get!(0xFE);
	info!("power monitor chip manufacturer ID: {:04X}", manuid);
	let dieid = get!(0xFF);
	info!("power monitor chip die ID: {:04X}", dieid);

	// Set the configuration value
	set!(
		0x00,
		Configuration::new()
			.with_average_samples(AverageSamples::Avg64)
			.with_bus_conversion_time(ConverstionTime::Us140)
			.with_shunt_conversion_time(ConverstionTime::Us140)
			.with_mode(Mode::ShuntAndBusContinuous)
	);

	// Set the calibration register. The board uses a 2mOhm shunt resistor.
	set!(0x05, 0x0A00u16);
	info!("calibrated power monitor chip");

	loop {
		Timer::after(Duration::from_millis(1000)).await;
		let current = get!(0x04);
		info!("powermon: current: {}mA", current);
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
#[repr(u16)]
#[allow(dead_code)]
enum AverageSamples {
	Avg1 = 0b000,
	Avg4 = 0b001,
	Avg16 = 0b010,
	Avg64 = 0b011,
	Avg128 = 0b100,
	Avg256 = 0b101,
	Avg512 = 0b110,
	Avg1024 = 0b111,
}

#[derive(Clone, Copy)]
#[repr(u16)]
#[allow(dead_code)]
enum ConverstionTime {
	Us140 = 0b000,
	Us204 = 0b001,
	Us332 = 0b010,
	Us588 = 0b011,
	Ms1p1 = 0b100,
	Ms2p116 = 0b101,
	Ms4p156 = 0b110,
	Ms8p244 = 0b111,
}

#[derive(Clone, Copy)]
#[repr(u16)]
#[allow(dead_code)]
enum Mode {
	PowerDown = 0b000,
	ShuntVoltageTriggered = 0b001,
	BusVoltageTriggered = 0b010,
	ShuntAndBusTriggered = 0b011,
	#[deprecated(note = "duplicate value; use PowerDown instead")]
	PowerDown2 = 0b100,
	ShuntVoltageContinuous = 0b101,
	BusVoltageContinuous = 0b110,
	ShuntAndBusContinuous = 0b111,
}
