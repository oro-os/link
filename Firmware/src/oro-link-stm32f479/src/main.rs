#![no_main]
#![no_std]

mod clock;
mod dbg;

use core::panic::PanicInfo;
use stm32f4xx_hal::{
	pac,
	prelude::*,
	serial::{Config, Serial},
};

struct Stm32F479;

impl oro_link_firmware::Arch for Stm32F479 {
	fn debug_write(s: &str) {
		self::dbg::write(s)
	}
}

#[panic_handler]
fn panic(_panic: &PanicInfo<'_>) -> ! {
	loop {}
}

#[no_mangle]
fn main() -> ! {
	let p = pac::Peripherals::take().unwrap();
	//let mut syscfg = p.SYSCFG.constrain();

	// Initialize the clock
	self::clock::init(&p.RCC);
	let clocks = p.RCC.constrain().cfgr.freeze();

	//let gpioa = p.GPIOA.split();
	//let mut gpiob = p.GPIOB.split();
	//let gpioc = p.GPIOC.split();
	//let mut gpiod = p.GPIOD.split();
	let gpioe = p.GPIOE.split();
	//let gpiof = p.GPIOF.split();

	//let mut indlights_scl = gpiob.pb6.into_alternate();
	//let mut indlights_sda = gpiob.pb7.into_alternate();
	//let mut indlights_en = gpiob.pb4.into_push_pull_output();

	//let mut syseth_miso = gpioa.pa6.into_alternate();
	//let mut syseth_mosi = gpioa.pa7.into_alternate();
	//let mut syseth_ss = gpioa.pa4.into_alternate();
	//let mut syseth_sck = gpioa.pa5.into_alternate();
	//let mut syseth_rst = gpiob.pb1.into_push_pull_output();
	//let mut syseth_int = gpiob.pb0.make_interrupt_source(&mut syscfg);
	//let mut syseth_en = gpioa.pa2.into_push_pull_output();
	//let mut syseth_xfrm_en = gpioa.pa3.into_push_pull_output();

	//let mut oled_mosi = gpioc.pc3.into_alternate();
	//let mut oled_ss = gpiob.pb9.into_alternate();
	//let mut oled_sck = gpiod.pd3.into_alternate();
	//let mut oled_rst = gpioc.pc13.into_push_pull_output();
	//let mut oled_dc = gpioc.pc14.into_push_pull_output();
	//let mut oled_en = gpioe.pe2.into_push_pull_output();

	//let mut exteth_miso = gpioc.pc11.into_alternate();
	//let mut exteth_mosi = gpioc.pc12.into_alternate();
	//let mut exteth_ss = gpioa.pa15.into_alternate();
	//let mut exteth_sck = gpioc.pc10.into_alternate();
	//let mut exteth_rst = gpiod.pd0.into_push_pull_output();
	//let mut exteth_xfrm_en = gpiod.pd2.into_push_pull_output();
	//let mut exteth_en = gpiod.pd7.into_push_pull_output();
	//let mut exteth_int = gpiod.pd1.make_interrupt_source(&mut syscfg);

	//let mut uart_rx = gpioe.pe7.into_alternate();
	let uart_tx = gpioe.pe8.into_alternate();

	//let mut rs232_cts = gpiob.pb13.into_alternate();
	//let mut rs232_rts = gpiob.pb14.into_alternate();
	//let mut rs232_rx = gpiob.pb11.into_alternate();
	//let mut rs232_tx = gpiob.pb10.into_alternate();
	//let mut rs232_en = gpiod.pd8.into_push_pull_output();

	//let mut usb_dn = gpioa.pa11.into_alternate();
	//let mut usb_dp = gpioa.pa12.into_alternate();

	//let mut sys_power = gpioc.pc8.into_push_pull_output();
	//let mut sys_reset = gpioc.pc9.into_push_pull_output();

	//let mut psu_standby = gpiod.pd4.into_push_pull_output();
	//let mut psu_on = gpiod.pd6.into_push_pull_output();
	//let mut psu_ok = gpiod.pd5.make_interrupt_source(&mut syscfg);

	let mut dbgled = gpioe.pe12.into_push_pull_output();

	self::dbg::init_dbg!(p.UART7, uart_tx, clocks);

	oro_link_firmware::main::<Stm32F479>();

	loop {
		dbgled.set_high();
		for _ in 0..1000000 {
			unsafe {
				::core::arch::asm!("NOP");
			}
		}
		dbgled.set_low();
		for _ in 0..1000000 {
			unsafe {
				::core::arch::asm!("NOP");
			}
		}
	}
}
