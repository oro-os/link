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
use tftp::{FileOperation, Message};

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
					Command::IncomingPacket(Packet::TftpBlock(bid, buf)) => {
						if let Some(ep) = last_ep {
							let msg = Message::Data(bid, buf.as_slice().try_into().unwrap());

							if let Ok(vec) = TryInto::<Vec<u8, 600>>::try_into(msg) {
								if let Err(err) = socket.send_to(vec.as_slice(), ep).await {
									error!("tftp: failed to send data block to SUT: {:?}", err);
								} else {
									trace!("tftp: transferred block {} of size {}", bid, buf.len());
								}
							} else {
								error!(
									"tftp: failed to send data block to SUT: constructed message was too large"
								);
							}
						}
					}
					Command::IncomingPacket(Packet::TftpError(bid, msg)) => {
						if let Some(ep) = last_ep {
							let msg = Message::Error(bid, msg.as_str());

							if let Ok(vec) = TryInto::<Vec<u8, 300>>::try_into(msg) {
								if let Err(err) = socket.send_to(vec.as_slice(), ep).await {
									error!("tftp: failed to send error message to SUT: {:?}", err);
								} else {
									trace!("tftp: transferred error to SUT");
								}
							} else {
								error!(
									"tftp: failed to send error message to SUT: constructed message was too large"
								);
							}
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

		let message = match Message::try_from(&buf[..n]) {
			Ok(msg) => msg,
			Err(err) => {
				warn!("tftp: invalid incoming message; dropping: {:?}", err);
				continue;
			}
		};

		broker_sender
			.send(Command::OutgoingPacket(match message {
				Message::File {
					operation,
					path,
					mode: _,
				} => match operation {
					FileOperation::Write => {
						warn!(
							"tftp: system under test sent a write file request unexpected: {:?}",
							path
						);
						continue;
					}
					FileOperation::Read => Packet::TftpRequest(path.into()),
				},
				Message::Data(_bid, _buf) => {
					warn!(
						"tftp: system under test sent a buffer of data to us unexpectedly; ignoring"
					);
					continue;
				}
				Message::Ack(bid) => Packet::TftpAck(bid),
				Message::Error(bid, msg) => Packet::TftpError(bid, msg.into()),
			}))
			.await;
	}
}
