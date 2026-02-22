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

#[derive(defmt::Format)]
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
	static ENC_BUFFER: StaticCell<[u8; link_protocol::stream::BUFFER_SIZE]> = StaticCell::new();
	let enc_buf = ENC_BUFFER.init([0; link_protocol::stream::BUFFER_SIZE]);

	let mut buf = [0u8; 64];

	'recover: loop {
		let mut limit = None;

		// Send a bunch of null bytes to reset the stream.
		defmt::debug!("writing 128 garbage bytes and 128 sentinels");
		uart_tx.write_all(&[0xFFu8; 128]).await.unwrap();
		uart_tx.write_all(&[0u8; 128]).await.unwrap();

		// Wait until we've read 128 null bytes in a row.
		defmt::debug!("waiting for 128 sentinels");
		let mut count = 0;
		'burn: for _ in 0..512 {
			let n = uart_rx.read(&mut buf).await.unwrap();
			for i in 0..n {
				if buf[i] == 0 {
					count += 1;
					if count == 128 {
						defmt::debug!("stream is reset; continuing");
						buf.copy_within((i + 1)..n, 0);
						limit = Some(n - (i + 1));
						break 'burn;
					}
				} else {
					count = 0;
				}
			}
		}

		if limit.is_none() {
			defmt::error!("failed to flush stream; waiting 1s and then restarting");
			Timer::after_secs(1).await;
			continue 'recover;
		}

		defmt::trace!("got leftover limit: {:?}", limit);

		// Reset the decoder. This feeds a sentinel value to it,
		// causing it to abort parsing. It'll reset the stream
		// and probably return an error, but we swallow it.
		decoder.feed(&[0]).ok();
		defmt::trace!("reset cobs decoder");

		loop {
			// Read the request
			let incoming = match limit.take() {
				Some(0) | None => {
					defmt::trace!("no leftovers; reading request bytes");
					uart_rx.read(&mut buf).await.unwrap()
				}
				Some(incoming) => incoming,
			};

			defmt::trace!("feeding byte length: {}", incoming);

			if incoming == 0 {
				defmt::trace!("0 length; starting new read");
				continue;
			}

			let report = match decoder.feed(&buf[..incoming]) {
				Ok(r) => r,
				Err(link_protocol::stream::StreamError::Empty) => {
					defmt::warn!("empty frame sentinel; skipping");
					continue;
				}
				Err(err) => {
					defmt::error!("stream decoder error: {:?}", err);
					continue 'recover;
				}
			};

			let Some(report) = report else {
				defmt::trace!("decoding not finished");
				continue;
			};

			defmt::trace!(
				"decoding finished; {} decoded, {} leftover",
				report.decoded_size,
				report.leftover
			);

			buf.copy_within((incoming - report.leftover)..incoming, 0);
			limit = Some(report.leftover);

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
			let (res, additional) = res.into_raw();
			let offlen = link_protocol::stream::encode_response(&res, enc_buf).unwrap();
			uart_tx
				.write_all(offlen.get_for_slice(enc_buf))
				.await
				.unwrap();
			uart_tx.flush().await.unwrap();

			if let Some(additional) = additional {
				defmt::trace!("sending additional data: {:?}", additional);

				match additional {
					AdditionalData::OledFrame => {
						let fb = super::dev_oled::FRAME_BUFFER.lock().await;
						let data = fb.data();
						uart_tx.write_all(data).await.unwrap();
						uart_tx.flush().await.unwrap();
					}
				}
			}

			continue;
		}
	}
}
