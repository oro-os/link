//! This defines the protocol for communication between the Link and the Daemon.
//! Messages are framed with a 16-bit unsigned length prefix.
#![no_std]
#![allow(clippy::large_enum_variant)]

#[cfg(feature = "defmt")]
use defmt::Format;

/// Messages sent from the Link to the daemon.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt", derive(Format))]
pub enum LinkMessage {
	/// An error occurred. The link should re-initialize and then reset some time
	/// after sending.
	Error([u8; 256]),

	/// The link is online and ready to receive work. Must be sent at least
	/// once per connection.
	LinkOnline {
		/// The link's 256 bit UID (should be a Sha256
		/// of the PAC/etc. UID chip readout).
		uid: [u8; 32],
		/// The link's firmware version
		version: [u8; 64],
	},

	/// Requests a file from the daemon during a TFTP PXE boot.
	TftpRead([u8; 256]),

	/// Any buffered output from the serial line of the SUT.
	SerialOut { count: u8, data: [u8; 256] },
}

/// Messages sent from the daemon to the Link.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt", derive(Format))]
pub enum DaemonMessage {
	/// The link should re-initialize and then reset once received.
	Kill9,

	/// The daemon will proceed to send a byte stream of the number of bytes
	/// directly after this message.
	BeginStream(u32),

	/// Begins a new test session. Any running session is cancelled,
	/// and the link re-initializes prior to running the test session.
	///
	/// The link should turn the machine on and begin the PXE booting process.
	BeginTestSession {
		/// The total number of tests that will be run
		total_tests: u32,
		/// The author to credit for the test session
		author: [u8; 256],
		/// The title of the test session
		title: [u8; 256],
		/// A technical (e.g. Git) reference ID for the test session
		ref_id: [u8; 256],
	},

	/// Sends data to the input of the SUT serial line.
	SerialIn { count: u8, data: [u8; 256] },
}
