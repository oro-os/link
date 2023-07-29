//! Debug output (UART7) handling

use core::fmt::{Arguments, Write};
use stm32f4xx_hal::{pac::UART7, serial::Tx};

static mut TX_SERIAL: Option<Tx<UART7, u8>> = None;

pub fn init(tx: Tx<UART7, u8>) {
	unsafe {
		TX_SERIAL = Some(tx);
	}
}

pub fn write(args: Arguments) {
	unsafe { TX_SERIAL.as_mut().unwrap() }
		.write_fmt(args)
		.unwrap();
}

macro_rules! init_dbg {
	($uart:expr, $tx_pin:expr, $clocks:expr) => {
		crate::dbg::init(
			::stm32f4xx_hal::serial::Serial::tx(
				$uart,
				$tx_pin,
				::stm32f4xx_hal::serial::Config::default()
					.baudrate(115200.bps())
					.wordlength_8()
					.stopbits(::stm32f4xx_hal::serial::config::StopBits::STOP1)
					.parity_none(),
				&$clocks,
			)
			.unwrap(),
		);
	};
}

pub(crate) use init_dbg;
