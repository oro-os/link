use stm32f4xx_hal::pac::RCC;

pub fn init(rcc: &RCC) {
	// turn on HSE
	rcc.cr.write(|w| w.hseon().set_bit());
	// wait for HSE to come online
	while rcc.cr.read().hserdy().bit() {}
	// configure prescalars
	rcc.cfgr.write(|w| unsafe {
		w.ppre1()
			.bits(0b101) // APB1 /4
			.ppre2()
			.bits(0b100) // APB2 /2
			.hpre()
			.bits(0b0000) // AHB /1
		// NOTE: MCO's are not used.
	});
	// configure main PLL
	rcc.pllcfgr.write(|w| unsafe {
		w.pllm()
			.bits(22) // PLLM /22
			.plln()
			.bits(285) // PLLN X285
			.pllp()
			.bits(0b00) // PLLP /2
			.pllr()
			.bits(0b10) // PLLR /2
			.pllq()
			.bits(7) // PLLQ /7
			.pllsrc()
			.hse() // use HSE for PLL
	});
	// set PLLQ as 48Mhz clock source
	rcc.dckcfgr.write(|w| w.ck48msel().pll());
	// enable PLL
	rcc.cr.write(|w| w.pllon().set_bit());
	// wait for PLL to come online
	while rcc.cr.read().pllrdy().bit() {}
}
