use defmt::{trace, warn};
use embassy_net::{
	driver::Driver,
	udp::{PacketMetadata, UdpSocket},
	Stack,
};
use smoltcp::wire::{EthernetAddress, IpAddress, IpEndpoint, Ipv4Address};
use tftp::{BufAtMost512, FileOperation, Message};

const TFTP_PORT: u16 = 69;

pub async fn run<D: Driver + 'static>(stack: &Stack<D>) -> ! {
	let mut rx_meta = [PacketMetadata::EMPTY; 16];
	let mut tx_meta = [PacketMetadata::EMPTY; 16];
	let mut rx_buffer = [0; 2048];
	let mut tx_buffer = [0; 2048];
	let mut buf = [0; 2048];

	let mut socket = UdpSocket::new(
		stack,
		&mut rx_meta,
		&mut rx_buffer,
		&mut tx_meta,
		&mut tx_buffer,
	);

	socket.bind(TFTP_PORT).unwrap();

	loop {
		let (n, ep) = socket.recv_from(&mut buf).await.unwrap();

		let message = match Message::try_from(&buf[..n]) {
			Ok(msg) => msg,
			Err(err) => {
				warn!("tftp: invalid incoming message; dropping: {:?}", err);
				continue;
			}
		};

		trace!("tftp: incoming packet: {:?}", message);
	}
}
