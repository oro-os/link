//! This defines the protocol for communication between the Link and the Daemon.
//! Messages are framed with a 16-bit unsigned length prefix.
#![no_std]
#![feature(async_fn_in_trait)]

use heapless::String;
use link_protocol_binser::LinkMessage;
pub use link_protocol_binser::{Deserialize, Error, Read, Serialize, Write};

#[cfg(feature = "defmt")]
use defmt::Format;

/// Messages sent from the Link to the daemon.
#[derive(Debug, Clone, LinkMessage)]
#[cfg_attr(feature = "defmt", derive(Format))]
#[non_exhaustive]
pub enum LinkPacket {
	/// The link is online and ready to receive work. Must be sent at least
	/// once per connection.
	#[proto(id = 1)]
	LinkOnline {
		/// The link's 256 bit UID (should be a Sha256
		/// of the PAC/etc. UID chip readout).
		uid: [u8; 32],
		/// The link's firmware version
		version: String<32>,
	},

	/// Resets the link, which is the equivalent of hitting the reset button.
	#[proto(id = 2)]
	ResetLink,
}
