//! The PXE system is a little complex due to several limitations in
//! Rust, Embassy, and some missing features. I won't get into the specifics,
//! but one of the constraints in order for the chip/board abstractions to work
//! is to keep the `#[embassy_executor::task]` functions in the _main.rs_ file,
//!
//! However, I don't want to complicate _main.rs_ with all of the channel plumbing
//! specifics, so instead I opaquely abstract them with "tokens" that are used to
//! invoke the individual tasks themselves from the main file, allowing for TAIT
//! types to propagate correctly without running into cross-module TAIT restrictions.
//!
//! I'm not explaining this well, I know, but this whole thing has been a giant headache
//! today and so far this is the cleanest way I can think about implementing this.

use crate::uc::RawEthernetDriver;
use defmt::debug;
use embassy_sync::{
	blocking_mutex::raw::NoopRawMutex,
	channel::{Channel, Receiver, Sender},
};
use smoltcp::wire::EthernetFrame;
use static_cell::make_static;

type EthChannel = Channel<NoopRawMutex, EthernetFrame<&'static [u8]>, 8>;
type EthSender = Sender<'static, NoopRawMutex, EthernetFrame<&'static [u8]>, 8>;
type EthReceiver = Receiver<'static, NoopRawMutex, EthernetFrame<&'static [u8]>, 8>;

macro_rules! select {
	($first_id:ident @ $first_future:expr => $first_block:block $second_id:ident @ $second_future:expr => $second_block:block) => {
		async move {
			match ::embassy_futures::select::select($first_future, $second_future).await {
				::embassy_futures::select::Either::First($first_id) => $first_block,
				::embassy_futures::select::Either::Second($second_id) => $second_block,
			}
		}
	};
}

pub async fn run_broker<D: RawEthernetDriver>(token: BrokerToken, mut driver: D) {
	debug!("starting net broker");

	let mut buf = &mut *make_static!([0u8; 2048]);

	loop {
		let recv_future = driver.recv(buf);
		let outgoing_future = token.outgoing.receive();

		select! {
			len @ recv_future => {
				debug!("broker: got recv'd packet: {}", len);
			}

			_msg @ outgoing_future => {
				debug!("broker: got outdoing packet");
			}
		}
		.await;
	}
}

pub async fn run_icmp(token: IcmpToken) {}

pub struct BrokerToken {
	outgoing: EthReceiver,
	icmp: EthSender,
}

pub struct IcmpToken(EthReceiver, EthSender);

pub struct PxeTokens {
	pub broker_token: BrokerToken,
	pub icmp_token: IcmpToken,
}

pub fn init_pxe() -> PxeTokens {
	let outgoing_channel = &*make_static!(EthChannel::new());
	let outgoing_sender = outgoing_channel.sender();
	let outgoing_receiver = outgoing_channel.receiver();

	let icmp_channel = &*make_static!(EthChannel::new());
	let icmp_sender = icmp_channel.sender();
	let icmp_receiver = icmp_channel.receiver();

	PxeTokens {
		broker_token: BrokerToken {
			outgoing: outgoing_receiver,
			icmp: icmp_sender,
		},
		icmp_token: IcmpToken(icmp_receiver, outgoing_sender),
	}
}
