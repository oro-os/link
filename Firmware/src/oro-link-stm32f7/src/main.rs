#![no_main]
#![no_std]
use core::panic::PanicInfo;
use stm32f7::stm32f7x6::Peripherals;

#[no_mangle]
fn main() -> ! {
	let per = Peripherals::take().unwrap();
	per.RCC.ahb1enr.write(|w| w.gpioben().bit(true));
	per.GPIOB.moder.write(|w| w.moder7().output());

	let mut on: bool = true;
	loop {
		per.GPIOB.odr.modify(|_, w| w.odr7().bit(on));
		on = !on;

		for _ in 0..1000000 {
			unsafe {
				core::arch::asm!("NOP");
			}
		}
	}
}

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
	loop {}
}
