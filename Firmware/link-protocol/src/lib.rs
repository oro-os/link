//! This defines the protocol for communication between the Link and the Daemon.
//! Messages are framed with a 16-bit unsigned length prefix.
#![cfg_attr(all(not(test), feature = "embedded-io"), no_std)]
#![feature(async_fn_in_trait)]

#[cfg(feature = "channels")]
pub mod channel;
#[cfg(feature = "channels")]
mod macros;

use heapless::String;
use link_protocol_binser::LinkMessage;
pub use link_protocol_binser::{Deserialize, Error, Read, Serialize, Write};

#[cfg(feature = "defmt")]
use defmt::Format;

/// Packets sent between the client and daemon.
#[derive(Debug, Clone, LinkMessage)]
#[cfg_attr(feature = "defmt", derive(Format))]
#[non_exhaustive]
pub enum Packet {
	/// The link is online and ready to receive work. Must be sent at least
	/// once per connection.
	#[proto(id = 1)]
	LinkOnline {
		/// The link's 256 bit UID (should be a Sha256
		/// of the PAC/etc. UID chip readout).
		uid: [u8; 32],
		/// The link's firmware version
		version: String<16>,
	},

	/// Resets the link, which is the equivalent of hitting the reset button.
	#[proto(id = 2)]
	ResetLink,
}
