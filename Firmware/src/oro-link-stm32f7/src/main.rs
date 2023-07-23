#![no_main]
#![no_std]
#![deprecated(
	note = "the stm32f7 firmware implementation of the Oro Link is primarily used for debugging via the Nucleo boards and is NOT a chip used in any of the boards themselves; you're probably looking for another implementation"
)]

use core::panic::PanicInfo;
use stm32f7xx_hal::{pac, prelude::*};

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
	loop {}
}

#[no_mangle]
fn main() -> ! {
	let p = pac::Peripherals::take().unwrap();

	let gpiob = p.GPIOB.split();
	let mut led0 = gpiob.pb0.into_push_pull_output();
	let mut led1 = gpiob.pb7.into_push_pull_output();
	let mut led2 = gpiob.pb14.into_push_pull_output();
	let gpioc = p.GPIOC.split();
	let button = gpioc.pc13.into_pull_down_input();

	let mut is_high = false;
	let mut num = 0;
	loop {
		if button.is_high() {
			if !is_high {
				is_high = true;
				led0.set_low();
				led1.set_low();
				led2.set_low();

				match num {
					0 => led0.set_high(),
					1 => led1.set_high(),
					2 => led2.set_high(),
					_ => unreachable!(),
				}

				num = (num + 1) % 3;
			}
		} else {
			is_high = false;
		}
	}
}
