use defmt::Format;
use embassy_sync::{
	blocking_mutex::raw::NoopRawMutex,
	channel::{Channel, Receiver, Sender},
};
use heapless::String;

pub type CommandChannel<const SZ: usize> = Channel<NoopRawMutex, Command, SZ>;
pub type CommandReceiver<const SZ: usize> = Receiver<'static, NoopRawMutex, Command, SZ>;
pub type CommandSender<const SZ: usize> = Sender<'static, NoopRawMutex, Command, SZ>;

#[derive(Format)]
#[non_exhaustive]
pub enum Command {
	/// Says hello to the daemon, bringing the link online and marking
	/// it as available.
	DaemonHello { uid: [u8; 32], version: String<16> },
	/// Resets the link
	Reset,
}
