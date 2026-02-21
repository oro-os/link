use embassy_stm32::{mode::Async, usart::Uart};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embedded_io_async::Write;
use link_protocol::minicbor::{self, encode::write::Cursor};
use static_cell::StaticCell;

pub type Channel = crate::channel::Channel<Cmd, 16>;

pub enum Cmd {
	Send(link_protocol::Response),
}

pub struct Config {
	pub uart: Uart<'static, Async>,
}

pub static PACKET: Signal<CriticalSectionRawMutex, link_protocol::Request> = Signal::new();

#[embassy_executor::task]
pub async fn run(rx: &'static Channel, config: Config) -> ! {
	let Config { mut uart } = config;

	static BUFFER: StaticCell<[u8; 4096]> = StaticCell::new();
	let buf = BUFFER.init([0u8; 4096]);

	loop {
		let mut len = [0u8; 4];
		uart.read(&mut len).await.unwrap();
		let len = u32::from_be_bytes(len);
		defmt::trace!("read length: {}", len);

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
				uart.read(&mut buf[0..n]).await.unwrap();
				to_read -= n;
			}

			let mut cursor = Cursor::new(&mut buf[..]);
			minicbor::encode(
				link_protocol::Response::Err(link_protocol::Error::TooLong),
				&mut cursor,
			)
			.unwrap();
			let position = cursor.position();
			uart.write(&buf[..position]).await.unwrap();
			continue;
		}

		uart.read(&mut buf[..len]).await.unwrap();
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
			uart.write(&buf[..position]).await.unwrap();
			continue;
		};

		defmt::debug!("received request: {:?}", request);

		PACKET.signal(request);

		let Cmd::Send(res) = rx.receive().await;
		defmt::debug!("sending response: {:?}", res);

		let mut cursor = Cursor::new(&mut buf[4..]);
		minicbor::encode(res, &mut cursor).unwrap();
		let position = cursor.position();
		let length = (position as u32).to_be_bytes();
		buf[..4].copy_from_slice(&length);
		uart.write_all(&buf[..position+4]).await.unwrap();
	}
}
