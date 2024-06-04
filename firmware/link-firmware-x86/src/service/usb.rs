use core::sync::atomic::{AtomicBool, Ordering};

use defmt::{info, warn};
use embassy_futures::join::join;
use embassy_time::{Duration, Timer};
use embassy_usb as usb;
use embassy_usb::class::hid::{HidReaderWriter, ReportId, RequestHandler, State};
use embassy_usb::control::OutResponse;
use embassy_usb::Handler;
use link_protocol::Packet;
use static_cell::make_static;
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

use crate::command::{Command, CommandReceiver, CommandSender};

pub async fn run<D: usb::driver::Driver<'static>>(
	mut builder: usb::Builder<'static, D>,
	_broker_sender: CommandSender<8>,
	usb_receiver: CommandReceiver<16>,
) -> ! {
	let request_handler = &mut *make_static!(MyRequestHandler {});
	let device_handler = &mut *make_static!(MyDeviceHandler::new());

	let state = &mut *make_static!(State::new());

	builder.handler(device_handler);

	// Create classes on the builder.
	let config = embassy_usb::class::hid::Config {
		report_descriptor: KeyboardReport::desc(),
		request_handler: None,
		poll_ms: 60,
		max_packet_size: 8,
	};

	let hid = HidReaderWriter::<_, 1, 8>::new(&mut builder, state, config);

	let mut usb = builder.build();

	// Wait for the USB peripheral to be ready.
	loop {
		let packet = usb_receiver.receive().await;
		if let Command::IncomingPacket(Packet::DebugUsbKey(keycode)) = packet {
			if keycode == 0 {
				info!("got usb boot signal");
				break;
			}

			warn!("invalid initial usb keycode (expecting 0): {:?}", keycode);
		} else {
			warn!("invalid packet received: {:?}", packet);
		}
	}

	info!("resuming usb");
	usb.wait_resume().await;

	let usb_fut = usb.run();

	let (reader, mut writer) = hid.split();

	// Do stuff with the class!
	let in_fut = async {
		loop {
			let packet = usb_receiver.receive().await;
			let Command::IncomingPacket(Packet::DebugUsbKey(keycode)) = packet else {
				warn!("invalid packet received: {:?}", packet);
				continue;
			};

			info!("pressing button: {:?}", keycode);

			// Create a report with the A key pressed. (no shift modifier)
			let report = KeyboardReport {
				keycodes: [keycode, 0, 0, 0, 0, 0],
				leds: 0,
				modifier: 0,
				reserved: 0,
			};

			match writer.write_serialize(&report).await {
				Ok(()) => {}
				Err(e) => warn!("failed to send report (down): {:?}", e),
			}

			Timer::after(Duration::from_millis(50)).await;

			let report = KeyboardReport {
				keycodes: [0, 0, 0, 0, 0, 0],
				leds: 0,
				modifier: 0,
				reserved: 0,
			};
			match writer.write_serialize(&report).await {
				Ok(()) => {}
				Err(e) => warn!("failed to send report (up): {:?}", e),
			};
		}
	};

	let out_fut = async { reader.run(false, request_handler).await };

	// Run everything concurrently.
	// If we had made everything `'static` above instead, we could do this using separate tasks instead.
	join(usb_fut, join(in_fut, out_fut)).await;
	unreachable!();
}

struct MyRequestHandler {}

impl RequestHandler for MyRequestHandler {
	fn get_report(&mut self, id: ReportId, _buf: &mut [u8]) -> Option<usize> {
		info!("Get report for {:?}", id);
		None
	}

	fn set_report(&mut self, id: ReportId, data: &[u8]) -> OutResponse {
		info!("Set report for {:?}: {=[u8]}", id, data);
		OutResponse::Accepted
	}

	fn set_idle_ms(&mut self, id: Option<ReportId>, dur: u32) {
		info!("Set idle rate for {:?} to {:?}", id, dur);
	}

	fn get_idle_ms(&mut self, id: Option<ReportId>) -> Option<u32> {
		info!("Get idle rate for {:?}", id);
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
			info!("Device enabled");
		} else {
			info!("Device disabled");
		}
	}

	fn reset(&mut self) {
		self.configured.store(false, Ordering::Relaxed);
		info!("Bus reset, the Vbus current limit is 100mA");
	}

	fn addressed(&mut self, addr: u8) {
		self.configured.store(false, Ordering::Relaxed);
		info!("USB address set to: {}", addr);
	}

	fn configured(&mut self, configured: bool) {
		self.configured.store(configured, Ordering::Relaxed);
		if configured {
			info!(
				"Device configured, it may now draw up to the configured current limit from Vbus."
			)
		} else {
			info!("Device is no longer configured, the Vbus current limit is 100mA.");
		}
	}
}
