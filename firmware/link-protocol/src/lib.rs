//! This defines the protocol for communication between the Link and the Daemon.
//! Messages are framed with a 16-bit unsigned length prefix.
//!
//! TODO: Implement `derive(LinkEnum)` which performs serialization/deserialization
//! TODO: based on discriminator, replace it for the trivial enums where `LinkMessage`
//! TODO: is currently being (ab)used. Alternatively, allow `LinkMessage` to pick up
//! TODO: on `#[repr(u8)]` etc. and enforce they are fieldless variants with unique
//! TODO: discriminators and do that instead. I don't remember why I didn't add
//! TODO: discriminators before, actually...
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::large_enum_variant, async_fn_in_trait)]

#[cfg(feature = "channels")]
pub mod channel;
#[cfg(feature = "channels")]
mod macros;

use heapless::{String, Vec};
use link_protocol_binser::LinkMessage;
pub use link_protocol_binser::{Deserialize, Error, Read, Serialize, Write};

/// Packets sent between the client and daemon.
#[derive(Debug, Clone, LinkMessage)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

	/// Sets the link's monitor scene
	#[proto(id = 3)]
	SetScene(Scene),

	/// Logs a message to the link's log scene
	#[proto(id = 4)]
	Log(LogEntry),

	/// Sets the monitor's standby mode; `true` will put the monitor
	/// into standby, ultimately (though perhaps not _immediately_)
	/// turning off the display. `false` turns the monitor back on.
	#[proto(id = 5)]
	SetMonitorStandby(bool),

	/// Starts a new test session
	#[proto(id = 6)]
	StartTestSession {
		total_tests: u32,
		author: String<255>,
		title: String<255>,
		ref_id: String<255>,
	},

	/// Starts a new test; no effect if a session isn't started.
	#[proto(id = 7)]
	StartTest { name: String<255> },

	/// Sets the power state of the machine
	#[proto(id = 8)]
	SetPowerState(PowerState),

	/// Sends the power signal to the machine
	#[proto(id = 9)]
	PressPower,

	/// Sends the reset signal to the machine
	#[proto(id = 10)]
	PressReset,

	/// Tells the PXE service how big the boot file size(s) are.
	/// **MUST** be sent before a test session is started (or at least
	/// before the system is turned on).
	#[proto(id = 12)]
	BootfileSize { uefi: u64, bios: u64 },

	/// A serial line transmission (either to or from the system)
	#[proto(id = 13)]
	Serial(Vec<u8, 256>),

	/// (DEBUG) An HID key for USB HID testing
	#[proto(id = 14)]
	DebugUsbKey(u8),
}

#[derive(Debug, Clone, LinkMessage)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Scene {
	#[proto(id = 1)]
	Logo,
	#[proto(id = 2)]
	Test,
	#[proto(id = 3)]
	Log,
}

#[derive(Debug, Clone, LinkMessage)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum LogEntry {
	#[proto(id = 1)]
	Info(String<255>),
	#[proto(id = 2)]
	Warn(String<255>),
	#[proto(id = 3)]
	Error(String<255>),
}

#[derive(Debug, Clone, LinkMessage)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum PowerState {
	#[proto(id = 1)]
	Off,
	#[proto(id = 2)]
	Standby,
	#[proto(id = 3)]
	On,
}
