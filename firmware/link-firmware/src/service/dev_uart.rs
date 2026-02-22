use embassy_stm32::{
	mode::Async,
	usart::{RingBufferedUartRx, UartTx},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embedded_io_async::Write;
use link_protocol::minicbor::{self, encode::write::Cursor};
use static_cell::StaticCell;

pub type Channel = crate::channel::Channel<Cmd, 16>;

pub enum Cmd {
	Send(Response),
}

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

	static BUFFER: StaticCell<[u8; 4096]> = StaticCell::new();
	let buf = BUFFER.init([0u8; 4096]);

	loop {
		let mut len = [0u8; 4];
		uart_rx.read(&mut len).await.unwrap();
		let len = u32::from_be_bytes(len);
		defmt::trace!("read length: {}", len);

		if len == 0 {
			defmt::warn!("got zero-length serial message");
			continue;
		}

		let (len, len_is_valid) = if len > (buf.len() as u32) {
			(0usize, false)
		} else {
			(len as usize, true)
		};

		if !len_is_valid {
			defmt::error!("received invalid message: too long");
			let mut to_read = len;
			while to_read > 0 {
				let n = to_read.min(buf.len());
				uart_rx.read(&mut buf[0..n]).await.unwrap();
				to_read -= n;
			}

			let mut cursor = Cursor::new(&mut buf[..]);
			minicbor::encode(
				link_protocol::Response::Err(link_protocol::Error::TooLong),
				&mut cursor,
			)
			.unwrap();
			let position = cursor.position();
			uart_tx.write(&buf[..position]).await.unwrap();
			continue;
		}

		uart_rx.read(&mut buf[..len]).await.unwrap();
		defmt::trace!("read {} bytes: {:X}", len, &buf[..len]);

		let Ok(request) = minicbor::decode(&buf[..len]) else {
			defmt::error!("received invalid message: could not decode");
			let mut cursor = Cursor::new(&mut buf[..]);
			minicbor::encode(
				link_protocol::Response::Err(link_protocol::Error::MalformedRequest),
				&mut cursor,
			)
			.unwrap();
			let position = cursor.position();
			uart_tx.write(&buf[..position]).await.unwrap();
			continue;
		};

		defmt::debug!("received request: {:?}", request);

		PACKET.signal(request);

		let Cmd::Send(res) = rx.receive().await;
		let (res, additional) = res.into_raw();
		defmt::debug!("sending response: {:?}", res);

		let mut cursor = Cursor::new(&mut buf[4..]);
		minicbor::encode(&res, &mut cursor).unwrap();
		let position = cursor.position();
		let length = (position as u32).to_be_bytes();
		buf[..4].copy_from_slice(&length);
		uart_tx.write_all(&buf[..position + 4]).await.unwrap();

		// Special handling; we have to do this for performance
		// reasons, as sending it as part of the response frame
		// is way too bulky for the protocol, and we don't have
		// a heap to work with.
		if matches!(additional, Some(AdditionalData::OledFrame)) {
			let fb = super::dev_oled::FRAME_BUFFER.lock().await;
			let data: &[u8; 256 * 64 / 2] = fb.data();
			uart_tx.write_all(data).await.unwrap();
		}
	}
}
