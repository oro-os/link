use embassy_stm32::{
	mode::Async,
	usart::{RingBufferedUartRx, UartTx},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embedded_io_async::Write;
use link_protocol::minicbor::{self, encode::write::Cursor};
use static_cell::StaticCell;

const MAX_FRAME_LEN: usize = 4096;
const RESET_REQ_WORD: u32 = 0xFFFF_FFFF;
const RESET_ACK_WORD: u32 = 0xFFFF_FFFE;

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

fn crc32(data: &[u8]) -> u32 {
	let mut crc = 0xFFFF_FFFFu32;
	for &byte in data {
		crc ^= byte as u32;
		for _ in 0..8 {
			let mask = (crc & 1).wrapping_neg();
			crc = (crc >> 1) ^ (0xEDB8_8320u32 & mask);
		}
	}
	!crc
}

async fn read_exact_rx(rx: &mut RingBufferedUartRx<'static>, dst: &mut [u8]) {
	let mut offset = 0;
	while offset < dst.len() {
		let bytes_read = rx.read(&mut dst[offset..]).await.unwrap();
		offset += bytes_read;
	}
}

async fn read_word(rx: &mut RingBufferedUartRx<'static>) -> u32 {
	let mut word = [0u8; 4];
	read_exact_rx(rx, &mut word).await;
	u32::from_be_bytes(word)
}

async fn write_word(tx: &mut UartTx<'static, Async>, word: u32) {
	tx.write_all(&word.to_be_bytes()).await.unwrap();
}

async fn write_response_frame(
	tx: &mut UartTx<'static, Async>,
	buf: &mut [u8],
	response: link_protocol::Response,
) {
	let mut cursor = Cursor::new(&mut buf[4..]);
	minicbor::encode(response, &mut cursor).unwrap();
	let payload_len = cursor.position();
	let payload = &buf[4..4 + payload_len];
	let payload_crc = crc32(payload).to_be_bytes();

	buf[..4].copy_from_slice(&(payload_len as u32).to_be_bytes());
	buf[4 + payload_len..8 + payload_len].copy_from_slice(&payload_crc);
	tx.write_all(&buf[..payload_len + 8]).await.unwrap();
}

async fn recover_link(rx: &mut RingBufferedUartRx<'static>, tx: &mut UartTx<'static, Async>) {
	write_word(tx, RESET_REQ_WORD).await;

	let mut window = 0u32;
	let mut filled = 0usize;
	let mut byte = [0u8; 1];

	loop {
		read_exact_rx(rx, &mut byte).await;
		window = (window << 8) | (byte[0] as u32);
		if filled < 3 {
			filled += 1;
			continue;
		}

		if window == RESET_REQ_WORD {
			write_word(tx, RESET_ACK_WORD).await;
			continue;
		}

		if window == RESET_ACK_WORD {
			return;
		}
	}
}

#[embassy_executor::task]
pub async fn run(rx: &'static Channel, config: Config) -> ! {
	let Config {
		mut uart_tx,
		mut uart_rx,
	} = config;

	static BUFFER: StaticCell<[u8; 4096]> = StaticCell::new();
	let buf = BUFFER.init([0u8; 4096]);

	loop {
		let len_word = read_word(&mut uart_rx).await;
		defmt::trace!("read length: {}", len_word);

		if len_word == RESET_REQ_WORD {
			defmt::warn!("received reset request");
			write_word(&mut uart_tx, RESET_ACK_WORD).await;
			continue;
		}

		if len_word == RESET_ACK_WORD {
			defmt::trace!("received unsolicited reset ack");
			continue;
		}

		if len_word == 0 || len_word > (MAX_FRAME_LEN as u32) {
			defmt::error!("received invalid frame length: {}", len_word);
			recover_link(&mut uart_rx, &mut uart_tx).await;
			continue;
		}

		let len = len_word as usize;
		read_exact_rx(&mut uart_rx, &mut buf[..len]).await;
		let received_crc = read_word(&mut uart_rx).await;

		let payload = &buf[..len];
		let expected_crc = crc32(payload);
		if received_crc != expected_crc {
			defmt::error!(
				"request CRC mismatch; expected {:X}, got {:X}",
				expected_crc,
				received_crc
			);
			recover_link(&mut uart_rx, &mut uart_tx).await;
			continue;
		}

		defmt::trace!("read {} bytes: {:X}", len, &buf[..len]);

		let Ok(request) = minicbor::decode(&buf[..len]) else {
			defmt::error!("received invalid message: could not decode request");
			recover_link(&mut uart_rx, &mut uart_tx).await;
			continue;
		};

		defmt::debug!("received request: {:?}", request);

		PACKET.signal(request);

		let Cmd::Send(res) = rx.receive().await;
		let (res, additional) = res.into_raw();
		defmt::debug!("sending response: {:?}", res);

		write_response_frame(&mut uart_tx, buf, res).await;

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
