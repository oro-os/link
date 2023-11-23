//! NOTE: The buffer sizes here and in the daemon protocol are 256, to match
//! NOTE: the STM's DMA buffer size. It's a tighter coupling than I would have liked,
//! NOTE: so if you're getting weird errors then a more proper decoupled fix may
//! NOTE: be necessary (e.g. having the chip implementation provide a constant buffer size
//! NOTE: or something, though not sure how that looks at the protocol level...)

use crate::{
	command::{Command, CommandReceiver, CommandSender},
	uc::{UartRx, UartTx},
};
use defmt::{trace, warn};
use embassy_futures::select::{select, Either};
use heapless::Vec;
use link_protocol::Packet;

pub async fn run<TX: UartTx, RX: UartRx, const S: usize, const RC: usize>(
	mut tx: TX,
	mut rx: RX,
	broker_sender: CommandSender<S>,
	serial_receiver: CommandReceiver<RC>,
) -> ! {
	loop {
		static mut RX_BUF: [u8; 256] = [0u8; 256];
		let rx_buf = unsafe { &mut RX_BUF[..] };

		let xmission = select(serial_receiver.receive(), async {
			match rx.read(rx_buf).await {
				Ok(len) => Some(len),
				Err(_err) => {
					// FIXME(qix-): For some reason none of these errors are defmt'able.
					warn!("serial: failed to read from serial (might be framing issue)");
					None
				}
			}
		})
		.await;

		match xmission {
			Either::First(Command::IncomingPacket(Packet::Serial(data))) => {
				trace!("serial: forwarding {} bytes to SUT", data.len());
				if let Err(_err) = tx.write_all(&data[..]).await {
					// FIXME(qix-): For some reason none of these errors are defmt'able.
					warn!("serial: failed to transmit to SUT");
				}
			}
			Either::First(unknown) => {
				warn!("serial: ignoring unknown command: {:?}", unknown);
			}
			Either::Second(Some(len)) => {
				trace!("serial: forwarding {} bytes to daemon", len);
				broker_sender
					.send(Command::OutgoingPacket(Packet::Serial(
						Vec::from_slice(&rx_buf[..len]).unwrap(),
					)))
					.await;
			}
			Either::Second(None) => {
				continue;
			}
		}
	}
}
