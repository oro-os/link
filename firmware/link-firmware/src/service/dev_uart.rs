use embassy_futures::select::{Either, select};
use embassy_stm32::{mode::Async, usart::Uart};
use heapless::Vec;

use super::Dispatch;
use crate::channel::{Channel as RawChannel, ChannelExt};

pub type Channel = RawChannel<Cmd, 16>;

pub const PACKET_SIZE: usize = 256;
pub type Packet = Vec<u8, PACKET_SIZE>;

#[derive(defmt::Format)]
#[allow(unused)]
pub enum Cmd {
	Send(Packet),
//	Recv(Packet),
//	RecvErr,
}

#[embassy_executor::task]
pub async fn run(
	recv: <Channel as ChannelExt>::Receiver,
	mut bus: super::Bus,
	mut uart: Uart<'static, Async>,
) -> ! {
	let mut buf = [0u8; PACKET_SIZE];
	loop {
		let r = select(uart.read_until_idle(&mut buf), recv.receive()).await;

		match r {
			Either::First(n) => {
				match n {
					Ok(n) => {
						let mut packet = Packet::new();
						packet.extend_from_slice(&buf[..n]).unwrap();
						defmt::trace!("uart recv {} bytes: {:?}", n, &packet);
						bus.dispatch(Message::Recv(packet)).await;
					}
					Err(e) => {
						defmt::error!("uart read error: {:?}", e);
						bus.dispatch(Message::RecvErr).await;
					}
				}
			}
			Either::Second(msg) => {
				match msg {
					Message::Send(packet) => {
						defmt::trace!("uart send {} bytes: {:?}", packet.len(), &packet);
						uart.write(&packet).await.unwrap();
					}
					_ => {
						panic!("unexpected message on uart service channel");
					}
				}
			}
		}
	}
}
