use crate::command::{Command, CommandReceiver, CommandSender};
use defmt::{error, trace, warn};
use embassy_futures::select::{select, Either};
use embassy_net::{
	driver::Driver,
	udp::{PacketMetadata, UdpSocket},
	Stack,
};
use heapless::Vec;
use link_protocol::Packet;

const TFTP_PORT: u16 = 69;

pub async fn run<D: Driver + 'static, const S: usize, const R: usize>(
	stack: &Stack<D>,
	broker_sender: CommandSender<S>,
	tftp_receiver: CommandReceiver<R>,
) -> ! {
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

	// This may not be the best way to do this, but it's the easiest
	// way I can think of.
	let mut last_ep = None;

	loop {
		let (n, ep) = match select(socket.recv_from(&mut buf), tftp_receiver.receive()).await {
			Either::First(Ok(n_ep)) => n_ep,
			Either::First(Err(err)) => {
				warn!("tftp: failed to receive packet from sut: {:?}", err);
				continue;
			}
			Either::Second(msg) => {
				match msg {
					Command::IncomingPacket(Packet::Tftp(data)) => {
						if let Some(ep) = last_ep {
							trace!("tftp: forwarding tftp packet to SUT of size {}", data.len());
							if let Err(err) = socket.send_to(data.as_slice(), ep).await {
								error!("tftp: failed to send tftp packet to SUT: {:?}", err);
							}
						} else {
							warn!(
								"tftp: daemon sent a tftp packet to SUT but SUT has not communicated with us yet; ignoring"
							);
						}
					}
					unknown => {
						warn!("tftp: ignoring unknown command: {:?}", unknown);
					}
				}
				continue;
			}
		};

		last_ep = Some(ep);

		let data = match Vec::from_slice(&buf[..n]) {
			Ok(d) => d,
			Err(err) => {
				error!(
					"tftp: failed to create tftp packet buffer with data from daemon for SUT: {:?}",
					err
				);
				continue;
			}
		};

		trace!(
			"tftp: forwarding tftp packet to daemon of size {}",
			data.len()
		);

		broker_sender
			.send(Command::OutgoingPacket(Packet::Tftp(data)))
			.await;
	}
}
