#![no_main]
#![no_std]
extern crate cortex_m;
extern crate cortex_m_rt as runtime;
extern crate stm32f7;

use core::panic::PanicInfo;
use cortex_m::asm;
use stm32f7::stm32f7x6::Peripherals;

#[no_mangle]
fn main() -> ! {
	let per = Peripherals::take().unwrap();

	// Enable the clock for GPIOB
	per.RCC.ahb1enr.write(|w| w.gpioben().bit(true));

	// Configure pin as output
	per.GPIOB.moder.write(|w| w.moder7().bits(0b01));

	// can't return so we go into an infinite loop here
	loop {
		// Toggle the LED output
		per.GPIOB
			.odr
			.modify(|r, w| w.odr7().bit(r.odr7().bit_is_clear()));

		for _i in 0..100000 {
			asm::nop()
		}
	}
}

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
	loop {}
}
