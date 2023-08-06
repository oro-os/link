use crate::uc;
use defmt::{info, Encoder, Logger};
use embassy_stm32::{
	bind_interrupts,
	dma::NoDma,
	gpio::{Input, Level, Output, OutputOpenDrain, Pull, Speed},
	i2c::{self, I2c},
	peripherals,
	spi::{self, Spi},
	time::Hertz,
	usart::{self, Uart},
	Config,
};
use embassy_time::Delay;
use embedded_hal_bus::spi::ExclusiveDevice;

bind_interrupts!(struct Irqs {
	I2C1_EV => i2c::InterruptHandler<peripherals::I2C1>;
	UART7 => usart::InterruptHandler<peripherals::UART7>;
});

trait ByteWriter {
	fn write(&mut self, buf: &[u8]);
	fn flush(&mut self);
}

type ImplWriter = impl ByteWriter;
static mut DEBUG_WRITE: Option<ImplWriter> = None;
static mut ENCODER: Encoder = Encoder::new();

#[defmt::global_logger]
struct DebugLogger;

unsafe impl Logger for DebugLogger {
	fn acquire() {
		unsafe {
			ENCODER.start_frame(|bytes| {
				if let Some(writer) = DEBUG_WRITE.as_mut() {
					writer.write(bytes);
				}
			})
		}
	}
	unsafe fn flush() {
		if let Some(writer) = DEBUG_WRITE.as_mut() {
			writer.flush();
		}
	}
	unsafe fn release() {
		unsafe {
			ENCODER.end_frame(|bytes| {
				if let Some(writer) = DEBUG_WRITE.as_mut() {
					writer.write(bytes);
					writer.flush();
				}
			})
		}
	}
	unsafe fn write(bytes: &[u8]) {
		ENCODER.write(bytes, |bytes| {
			if let Some(writer) = DEBUG_WRITE.as_mut() {
				writer.write(bytes);
			}
		});
	}
}

pub fn init() -> (
	impl uc::DebugLed,
	impl uc::SystemUnderTest,
	impl uc::IndicatorLights,
	impl uc::EthernetDriver,
) {
	let mut config = Config::default();
	config.rcc.hse = Some(Hertz::mhz(26));
	config.rcc.bypass_hse = false;
	config.rcc.hclk = Some(Hertz(168409091));
	config.rcc.sys_ck = Some(Hertz(168409091));
	config.rcc.pll48 = true;

	let p = embassy_stm32::init(config);

	let debug_write = Uart::new(p.UART7, p.PE7, p.PE8, Irqs, NoDma, NoDma, {
		let mut config = usart::Config::default();
		config.baudrate = 115200;
		config.data_bits = usart::DataBits::DataBits8;
		config.stop_bits = usart::StopBits::STOP1;
		config.parity = usart::Parity::ParityNone;
		config
	});

	unsafe {
		fn init(v: ImplWriter) -> Option<ImplWriter> {
			Some(v)
		}
		DEBUG_WRITE = init(debug_write);
	}

	info!("initializing STM32f479vg...");

	let mut ind_on = Output::new(p.PB4, Level::Low, Speed::Low);
	ind_on.set_high();
	::core::mem::forget(ind_on); // Keep it high even after we return.

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

	let indicators = crate::chip::is31fl3218::Is31fl3218::new(i2c);
	info!("... indicators INIT");

	let mut exteth_en = Output::new(p.PD7, Level::Low, Speed::Low);
	let mut exteth_xfrm_en = Output::new(p.PD2, Level::Low, Speed::Low);
	exteth_en.set_high();
	exteth_xfrm_en.set_high();
	// Keep them high even after we return.
	::core::mem::forget(exteth_en);
	::core::mem::forget(exteth_xfrm_en);

	info!("... external ethernet transformer INIT");

	let mut extconf = spi::Config::default();
	extconf.mode = spi::MODE_0;
	extconf.bit_order = spi::BitOrder::MsbFirst;
	extconf.frequency = Hertz(8_000_000);

	let extspi = Spi::new(p.SPI3, p.PC10, p.PC12, p.PC11, NoDma, NoDma, extconf);

	info!("... external ethernet comms INIT");

	let extdev = ExclusiveDevice::new(
		extspi,
		OutputOpenDrain::new(p.PA15, Level::High, Speed::VeryHigh, Pull::None),
		Delay,
	);

	info!("... external ethernet dev INIT");

	let exteth = crate::chip::enc28j60::Enc28j60::new(
		extdev,
		Some(Output::new(p.PD0, Level::High, Speed::VeryHigh)),
		[b'.', b'o', b'O', b'D', b'E', b'V'],
	);

	info!("... external ethernet INIT");

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
		exteth,
	)
}

impl<'d, T: usart::BasicInstance, TxDma, RxDma> ByteWriter for Uart<'d, T, TxDma, RxDma> {
	fn write(&mut self, buf: &[u8]) {
		self.blocking_write(buf).unwrap();
	}

	fn flush(&mut self) {
		self.blocking_flush().unwrap();
	}
}
