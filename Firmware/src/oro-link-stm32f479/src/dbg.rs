//! Debug output (UART7) handling

use core::fmt::Write;
use stm32f4xx_hal::{
	gpio::PushPull,
	pac::UART7,
	rcc::Clocks,
	serial::{Config, Instance, Serial, Tx},
};

static mut TX_SERIAL: Option<Tx<UART7, u8>> = None;

pub fn init(tx: Tx<UART7, u8>) {
	unsafe {
		TX_SERIAL = Some(tx);
	}
}

pub fn write(s: &str) {
	unsafe { TX_SERIAL.as_mut().unwrap() }.write_str(s).unwrap();
}

macro_rules! init_dbg {
	($uart:expr, $tx_pin:expr, $clocks:expr) => {
		crate::dbg::init(
			Serial::tx(
				$uart,
				$tx_pin,
				Config::default()
					.baudrate(115200.bps())
					.wordlength_8()
					.parity_none(),
				&$clocks,
			)
			.unwrap(),
		);
	};
}

pub(crate) use init_dbg;
