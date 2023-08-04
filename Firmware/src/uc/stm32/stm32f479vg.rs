use crate::uc;
use embassy_stm32::{
	bind_interrupts,
	dma::NoDma,
	gpio::{Input, Level, Output, Pull, Speed},
	i2c::{self, I2c},
	peripherals,
	time::Hertz,
	Config,
};

bind_interrupts!(struct Irqs {
	I2C1_EV => i2c::InterruptHandler<peripherals::I2C1>;
});

pub fn init() -> (
	impl uc::DebugLed,
	impl uc::SystemUnderTest,
	impl uc::IndicatorLights,
) {
	let mut config = Config::default();
	config.rcc.hse = Some(Hertz::mhz(26));
	config.rcc.bypass_hse = false;
	config.rcc.hclk = Some(Hertz(168409091));
	config.rcc.sys_ck = Some(Hertz(168409091));
	config.rcc.pll48 = true;

	let p = embassy_stm32::init(config);

	let mut ind_on = Output::new(p.PB4, Level::Low, Speed::Low);

	let i2c = I2c::new(
		p.I2C1,
		p.PB6,
		p.PB7,
		Irqs,
		NoDma,
		NoDma,
		Hertz(400_000),
		Default::default(),
	);

	ind_on.set_high();
	let indicators = crate::chip::is31fl3218::Is31fl3218::new(i2c);

	(
		super::DebugLed::new(Output::new(p.PE12, Level::Low, Speed::Low)),
		super::SystemUnderTest::new(
			Output::new(p.PC9, Level::Low, Speed::Low),
			Output::new(p.PC8, Level::Low, Speed::Low),
			Output::new(p.PD6, Level::Low, Speed::Low),
			Output::new(p.PD4, Level::Low, Speed::Low),
			Input::new(p.PD5, Pull::Up),
		),
		super::Is31fl3218IndicatorLights::<_, 0, 1, 17, 12, 13, 11, 16, 14, 15>::new(indicators),
	)
}
