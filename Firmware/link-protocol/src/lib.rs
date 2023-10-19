//! This defines the protocol for communication between the Link and the Daemon.
//! Messages are framed with a 16-bit unsigned length prefix.
#![no_std]

use link_protocol_binser::LinkMessage;
pub use link_protocol_binser::{Deserialize, Error, Serialize};

#[cfg(feature = "defmt")]
use defmt::Format;

/// Messages sent from the Link to the daemon.
#[derive(Debug, Clone, LinkMessage)]
#[cfg_attr(feature = "defmt", derive(Format))]
pub enum LinkPacket<'a> {
	/// An error occurred. The link should re-initialize and then reset some time
	/// after sending.
	#[proto(id = 0)]
	Error(&'a str),

	/// The link is online and ready to receive work. Must be sent at least
	/// once per connection.
	#[proto(id = 1)]
	LinkOnline {
		/// The link's 256 bit UID (should be a Sha256
		/// of the PAC/etc. UID chip readout).
		uid: [u8; 32],
		/// The link's firmware version
		version: &'a str,
	},

	/// Requests a file from the daemon during a TFTP PXE boot.
	#[proto(id = 2)]
	TftpRead(&'a str),

	/// Any buffered output from the serial line of the SUT.
	#[proto(id = 3)]
	SerialOut(&'a [u8]),

	/// The link should re-initialize and then reset once received.
	#[proto(id = 4)]
	Kill9,

	/// The daemon will proceed to send a byte stream of the number of bytes
	/// directly after this message.
	#[proto(id = 5)]
	BeginStream(u32),

	/// Begins a new test session. Any running session is cancelled,
	/// and the link re-initializes prior to running the test session.
	///
	/// The link should turn the machine on and begin the PXE booting process.
	#[proto(id = 6)]
	BeginTestSession {
		/// The total number of tests that will be run
		total_tests: u32,
		/// The author to credit for the test session
		author: &'a str,
		/// The title of the test session
		title: &'a str,
		/// A technical (e.g. Git) reference ID for the test session
		ref_id: &'a str,
	},

	/// Sends data to the input of the SUT serial line.
	#[proto(id = 7)]
	SerialIn(&'a [u8]),
}
