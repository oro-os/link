use core::{
	ptr::addr_of_mut,
	sync::atomic::{AtomicBool, Ordering},
};

use embassy_futures::join::join;
use embassy_stm32::{gpio::Output, peripherals, usb::Driver};
use embassy_time::{Duration, Timer};
use embassy_usb::{
	Builder, Handler,
	class::hid::{HidReaderWriter, ReportId, RequestHandler, State},
	control::OutResponse,
};
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

pub struct Config {
	pub driver:   Driver<'static, peripherals::USB_OTG_HS>,
	pub ulpi_rst: Output<'static>,
}

#[embassy_executor::task]
pub async fn run(config: Config) -> ! {
	let Config {
		driver,
		mut ulpi_rst,
	} = config;

	defmt::info!("resetting ULPI PHY");
	ulpi_rst.set_low();
	Timer::after(Duration::from_millis(10)).await;
	ulpi_rst.set_high();
	Timer::after(Duration::from_millis(10)).await;
	defmt::info!("ULPI reset; starting USB driver");

	let _state = State::new();

	let mut config = embassy_usb::Config::new(0xC0DE, 0xCAFF);
	config.manufacturer = Some("Oro Operating System");
	config.product = Some("Oro Link");
	config.serial_number = Some("OROOROOROOROOROORO");

	// Required for windows compatibility.
	// https://developer.nordicsemi.com/nRF_Connect_SDK/doc/1.9.1/kconfig/CONFIG_CDC_ACM_IAD.html#help
	config.device_class = 0xEF;
	config.device_sub_class = 0x02;
	config.device_protocol = 0x01;
	config.composite_with_iads = true;

	static mut CONFIG_DESCRIPTOR: [u8; 256] = [0; 256];
	static mut BOS_DESCRIPTOR: [u8; 256] = [0; 256];
	// You can also add a Microsoft OS descriptor.
	static mut MSOS_DESCRIPTOR: [u8; 256] = [0; 256];
	static mut CONTROL_BUF: [u8; 64] = [0; 64];

	let mut builder = unsafe {
		Builder::new(
			driver,
			config,
			&mut *addr_of_mut!(CONFIG_DESCRIPTOR),
			&mut *addr_of_mut!(BOS_DESCRIPTOR),
			&mut *addr_of_mut!(MSOS_DESCRIPTOR),
			&mut *addr_of_mut!(CONTROL_BUF),
		)
	};

	static REQUEST_HANDLER: static_cell::StaticCell<MyRequestHandler> =
		static_cell::StaticCell::new();
	static DEVICE_HANDLER: static_cell::StaticCell<MyDeviceHandler> =
		static_cell::StaticCell::new();
	let request_handler = REQUEST_HANDLER.init(MyRequestHandler {});
	let device_handler = DEVICE_HANDLER.init(MyDeviceHandler::new());

	static STATE: static_cell::StaticCell<State<'static>> = static_cell::StaticCell::new();
	let state = STATE.init(State::<'static>::new());

	builder.handler(device_handler);

	// Create classes on the builder.
	let config = embassy_usb::class::hid::Config {
		report_descriptor: KeyboardReport::desc(),
		request_handler:   None,
		poll_ms:           60,
		max_packet_size:   8,
		hid_boot_protocol: embassy_usb::class::hid::HidBootProtocol::Keyboard,
		hid_subclass:      embassy_usb::class::hid::HidSubclass::No,
	};

	let hid = HidReaderWriter::<_, 1, 8>::new(&mut builder, state, config);

	let mut usb = builder.build();

	// Wait for the USB peripheral to be ready.
	Timer::after(Duration::from_millis(1000)).await;

	defmt::info!("resuming usb");
	usb.wait_resume().await;

	let usb_fut = usb.run();

	let (reader, mut writer) = hid.split();

	// Do stuff with the class!
	let in_fut = async {
		loop {
			// Create a report with the A key pressed. (no shift modifier)
			let report = KeyboardReport {
				keycodes: [0x04, 0, 0, 0, 0, 0],
				leds:     0,
				modifier: 0,
				reserved: 0,
			};

			match writer.write_serialize(&report).await {
				Ok(()) => {}
				Err(e) => defmt::warn!("failed to send report (down): {:?}", e),
			}

			Timer::after(Duration::from_millis(50)).await;

			let report = KeyboardReport {
				keycodes: [0, 0, 0, 0, 0, 0],
				leds:     0,
				modifier: 0,
				reserved: 0,
			};
			match writer.write_serialize(&report).await {
				Ok(()) => {}
				Err(e) => defmt::warn!("failed to send report (up): {:?}", e),
			};

			Timer::after(Duration::from_millis(1000)).await;
		}
	};

	let out_fut = async { reader.run(false, request_handler).await };

	// Run everything concurrently.
	// If we had made everything `'static` above instead, we could do this using separate tasks instead.
	join(usb_fut, join(in_fut, out_fut)).await;
	panic!("usb task ended!");
}

struct MyRequestHandler {}

impl RequestHandler for MyRequestHandler {
	fn get_report(&mut self, id: ReportId, _buf: &mut [u8]) -> Option<usize> {
		defmt::info!("Get report for {:?}", id);
		None
	}

	fn set_report(&mut self, id: ReportId, data: &[u8]) -> OutResponse {
		defmt::info!("Set report for {:?}: {=[u8]}", id, data);
		OutResponse::Accepted
	}

	fn set_idle_ms(&mut self, id: Option<ReportId>, dur: u32) {
		defmt::info!("Set idle rate for {:?} to {:?}", id, dur);
	}

	fn get_idle_ms(&mut self, id: Option<ReportId>) -> Option<u32> {
		defmt::info!("Get idle rate for {:?}", id);
		None
	}
}

struct MyDeviceHandler {
	configured: AtomicBool,
}

impl MyDeviceHandler {
	fn new() -> Self {
		MyDeviceHandler {
			configured: AtomicBool::new(false),
		}
	}
}

impl Handler for MyDeviceHandler {
	fn enabled(&mut self, enabled: bool) {
		self.configured.store(false, Ordering::Relaxed);
		if enabled {
			defmt::info!("Device enabled");
		} else {
			defmt::info!("Device disabled");
		}
	}

	fn reset(&mut self) {
		self.configured.store(false, Ordering::Relaxed);
		defmt::info!("Bus reset, the Vbus current limit is 100mA");
	}

	fn addressed(&mut self, addr: u8) {
		self.configured.store(false, Ordering::Relaxed);
		defmt::info!("USB address set to: {}", addr);
	}

	fn configured(&mut self, configured: bool) {
		self.configured.store(configured, Ordering::Relaxed);
		if configured {
			defmt::info!(
				"Device configured, it may now draw up to the configured current limit from Vbus."
			)
		} else {
			defmt::info!("Device is no longer configured, the Vbus current limit is 100mA.");
		}
	}
}
