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
	per.RCC.ahb1enr.write(|w| w.gpioben().bit(true));
	per.GPIOB.moder.write(|w| w.moder7().bits(0b01));

	let mut on: bool = true;
	loop {
		per.GPIOB.odr.modify(|_, w| w.odr7().bit(on));
		on = !on;

		for _i in 0..50000 {
			asm::nop()
		}
	}
}

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
	loop {}
}
