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
//!
//! TODO: Implement copy-free packet handling.

mod packet;

use crate::uc::RawEthernetDriver;
use defmt::{debug, trace};
use embassy_futures::select::{select, Either};
use embassy_sync::{
	blocking_mutex::raw::NoopRawMutex,
	channel::{Channel, Receiver, Sender},
};
use heapless::Vec;
use smoltcp::wire::{
	EthernetAddress, EthernetFrame, EthernetProtocol, Icmpv6Message, Icmpv6Packet, IpProtocol,
	Ipv6Packet,
};
use static_cell::make_static;

const BUFFER_SIZE: usize = 2048;
type Buffer = Vec<u8, BUFFER_SIZE>;

type EthChannel = Channel<NoopRawMutex, Buffer, 2>;
type EthSender = Sender<'static, NoopRawMutex, Buffer, 2>;
type EthReceiver = Receiver<'static, NoopRawMutex, Buffer, 2>;

pub async fn run_broker<D: RawEthernetDriver>(token: BrokerToken, mut driver: D) {
	debug!("starting net broker");

	static mut BUFFER: [u8; BUFFER_SIZE] = [0u8; BUFFER_SIZE];

	loop {
		let result = async {
			let recv_future = driver.recv(unsafe { &mut BUFFER[..] });
			let outgoing_future = token.outgoing.receive();
			select(recv_future, outgoing_future).await
		}
		.await;

		match result {
			Either::First(len) => {
				if let Ok(frame) = EthernetFrame::new_checked(unsafe { &BUFFER[..len] }) {
					match frame.ethertype() {
						EthernetProtocol::Ipv6 => {
							if let Ok(ipv6_frame) = Ipv6Packet::new_checked(frame.payload()) {
								match ipv6_frame.next_header() {
									IpProtocol::Icmpv6 => {
										let copy =
											Buffer::from_slice(unsafe { &BUFFER[..len] }).unwrap();
										token.icmp.send(copy).await;
									}
									proto => {
										trace!(
											"broker dropping unsupported ipv6 frame ({:?})",
											proto
										);
									}
								}
							} else {
								trace!(
									"broker dropping too-small ipv6 frame ({})",
									frame.payload().len()
								);
							}
						}
						etype => {
							trace!("broker dropping unsupported ethertype frame ({:?})", etype);
						}
					}
				} else {
					trace!("broker dropping too-small ethernet frame ({})", len);
				}
			}

			Either::Second(msg) => {
				trace!("sending packet with length {}", msg.len());
				driver.send(&msg[..]).await;
			}
		}
	}
}

pub async fn run_icmpv6(token: Icmpv6Token, device_address: EthernetAddress) {
	loop {
		let buffer = token.0.receive().await;
		let frame = EthernetFrame::new_checked(&buffer[..]).unwrap();
		debug_assert_eq!(frame.ethertype(), EthernetProtocol::Ipv6);
		let ipv6_frame = Ipv6Packet::new_checked(frame.payload()).unwrap();
		debug_assert_eq!(ipv6_frame.next_header(), IpProtocol::Icmpv6);

		if let Ok(icmpv6_frame) = Icmpv6Packet::new_checked(ipv6_frame.payload()) {
			match icmpv6_frame.msg_type() {
				Icmpv6Message::RouterSolicit => {
					debug!("system sent router solicitation; sending advertisement");
					let mut res_buffer = Buffer::new();
					unsafe {
						res_buffer.set_len(res_buffer.capacity());
					}
					let len =
						packet::icmpv6_router_advertisement(&mut res_buffer[..], device_address);
					unsafe {
						res_buffer.set_len(len);
					}
					token.1.send(res_buffer).await;
				}

				mtype => {
					trace!("icmpv6 dropping unsupported message type ({:?})", mtype);
				}
			}
		} else {
			trace!("icmpv6 dropping too-short icmpv6 packet ({})", buffer.len());
		}
	}
}

pub struct BrokerToken {
	outgoing: EthReceiver,
	icmp: EthSender,
}

pub struct Icmpv6Token(EthReceiver, EthSender);

pub struct PxeTokens {
	pub broker_token: BrokerToken,
	pub icmpv6_token: Icmpv6Token,
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
		icmpv6_token: Icmpv6Token(icmp_receiver, outgoing_sender),
	}
}
