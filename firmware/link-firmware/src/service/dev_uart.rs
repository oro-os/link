use embassy_stm32::{
	mode::Async,
	usart::{RingBufferedUartRx, UartTx},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Timer;
use embedded_io_async::Write;
use static_cell::StaticCell;

pub type Channel = crate::channel::Channel<Cmd, 16>;

pub enum Cmd {
	Send(Response),
}

#[derive(defmt::Format)]
pub enum Response {
	Protocol(link_protocol::Response),
	OledFrame,
}

impl Response {
	fn into_raw(self) -> (link_protocol::Response, Option<AdditionalData>) {
		match self {
			Self::OledFrame => {
				(
					link_protocol::Response::BulkTransfer(256 * 64 / 2),
					Some(AdditionalData::OledFrame),
				)
			}
			Self::Protocol(res) => (res, None),
		}
	}
}

impl From<link_protocol::Response> for Response {
	#[inline]
	fn from(value: link_protocol::Response) -> Self {
		Self::Protocol(value)
	}
}

enum AdditionalData {
	OledFrame,
}

pub struct Config {
	pub uart_rx: RingBufferedUartRx<'static>,
	pub uart_tx: UartTx<'static, Async>,
}

pub static PACKET: Signal<CriticalSectionRawMutex, link_protocol::Request> = Signal::new();

#[embassy_executor::task]
pub async fn run(rx: &'static Channel, config: Config) -> ! {
	let Config {
		mut uart_tx,
		mut uart_rx,
	} = config;

	static BUFFER: StaticCell<[u8; link_protocol::stream::BUFFER_SIZE]> = StaticCell::new();
	let buf = BUFFER.init([0; link_protocol::stream::BUFFER_SIZE]);
	let mut decoder = link_protocol::stream::Decoder::new(buf);

	let mut pending_response: Option<Response> = None;

	let mut buf = [0u8; 64];

	'recover: loop {
		let mut bounds = None;

		// Send a bunch of null bytes to reset the stream.
		defmt::debug!("writing 128 sentinels");
		uart_tx.write_all(&[0u8; 128]).await.unwrap();

		// Wait until we've read 128 null bytes in a row.
		defmt::debug!("waiting for 128 sentinels");
		let mut count = 0;
		'burn: for _ in 0..32 {
			let n = uart_rx.read(&mut buf).await.unwrap();
			for i in 0..n {
				if buf[i] == 0 {
					count += 1;
					if count == 128 {
						defmt::debug!("stream is reset; continuing");
						bounds = Some((i + 1)..);
						break 'burn;
					}
				} else {
					count = 0;
				}
			}
		}

		if bounds.is_none() {
			defmt::error!("failed to flush stream; waiting 1s and then restarting");
			Timer::after_secs(1).await;
			continue 'recover;
		}

		defmt::trace!("got leftover bounds: {:?}", bounds);

		// Reset the decoder. This feeds a sentinel value to it,
		// causing it to abort parsing. It'll reset the stream
		// and probably return an error, but we swallow it.
		decoder.feed(&[0]).ok();
		defmt::trace!("reset cobs decoder");

		loop {
			if let Some(res) = &pending_response {
				defmt::trace!("(re)trying response");
				// Try to send it
				pending_response.take();
				// TODO
				continue;
			}

			// Read the request
			let incoming = if let Some(bounds) = bounds.take()
				&& bounds.clone().count() > 0
			{
				defmt::trace!("took bounds; reading leftovers: {:?}", bounds);
				&buf[bounds]
			} else {
				defmt::trace!("no leftovers; reading request bytes");
				let n = uart_rx.read(&mut buf).await.unwrap();
				&buf[..n]
			};

			defmt::trace!("feeding byte length: {}", incoming.len());

			if incoming.len() == 0 {
				defmt::warn!("read 0 bytes from uart");
				continue;
			}

			let Some(report) = decoder.feed(incoming).unwrap() else {
				defmt::trace!("decoding not finished");
				continue;
			};

			defmt::trace!(
				"decoding finished; {} decoded, {} leftover",
				report.decoded_size,
				report.leftover
			);

			bounds = Some((incoming.len() - report.leftover)..);

			let req = match decoder.decode_request() {
				Ok(r) => r,
				Err(err) => {
					defmt::warn!("invalid incoming request: {:?}", err);
					continue 'recover;
				}
			};

			defmt::trace!(
				"decoded incoming request; signaling and waiting for response: {:?}",
				req
			);

			PACKET.signal(req);

			// Wait for a response
			let Cmd::Send(res) = rx.receive().await;
			defmt::trace!("got response: {:?}", res);
			pending_response = Some(res);
			continue;
		}
	}
}
