use defmt::{debug, warn};
use embassy_net::{
	driver::Driver,
	udp::{PacketMetadata, UdpSocket},
	Stack,
};
use smoltcp::wire::{EthernetAddress, IpAddress, IpEndpoint, Ipv4Address};

const TFTP_PORT: u16 = 69;

pub async fn run<D: Driver + 'static>(stack: &Stack<D>) -> ! {
	let mut rx_meta = [PacketMetadata::EMPTY; 16];
	let mut tx_meta = [PacketMetadata::EMPTY; 16];
	let mut rx_buffer = [0; 2048];
	let mut tx_buffer = [0; 2048];
	let mut buf = [0; 2048];

	let mut socket = unsafe {
		UdpSocket::new(
			stack,
			&mut rx_meta,
			&mut rx_buffer,
			&mut tx_meta,
			&mut tx_buffer,
		)
	};

	socket.bind(TFTP_PORT).unwrap();

	loop {
		let (n, _) = socket.recv_from(&mut buf).await.unwrap();
	}
}
