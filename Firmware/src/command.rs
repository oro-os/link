#![allow(clippy::large_enum_variant)]

use crate::uc;
use defmt::Format;
use embassy_sync::{
	blocking_mutex::raw::NoopRawMutex,
	channel::{Channel, Receiver, Sender},
};
use heapless::String;
use link_protocol::Packet;

pub type CommandChannel<const SZ: usize> = Channel<NoopRawMutex, Command, SZ>;
pub type CommandReceiver<const SZ: usize> = Receiver<'static, NoopRawMutex, Command, SZ>;
pub type CommandSender<const SZ: usize> = Sender<'static, NoopRawMutex, Command, SZ>;

#[derive(Format)]
#[non_exhaustive]
pub enum Command {
	/// A new daemon connection has been established
	DaemonConnected,
	/// An incoming packet for processing
	IncomingPacket(Packet),
	/// An outgoing packet for sending to the daemon
	OutgoingPacket(Packet),
	/// Resets the link
	Reset,
	/// Changes the currently displayed scene
	SetScene(uc::Scene),
	/// Logs a message to the monitor
	Log(uc::LogFrame),
	/// Sets the monitor standby mode
	SetStandby(bool),
	/// Starts a new test session
	StartTestSession {
		total_tests: usize,
		author: String<255>,
		title: String<255>,
		ref_id: String<255>,
	},
	/// Starts a new test
	StartTest { name: String<255> },
}
