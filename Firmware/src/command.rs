use defmt::Format;
use embassy_sync::{
	blocking_mutex::raw::NoopRawMutex,
	channel::{Channel, Receiver, Sender},
};

pub type CommandChannel<const SZ: usize> = Channel<NoopRawMutex, Command, SZ>;
pub type CommandReceiver<const SZ: usize> = Receiver<'static, NoopRawMutex, Command, SZ>;
pub type CommandSender<const SZ: usize> = Sender<'static, NoopRawMutex, Command, SZ>;

#[derive(Format)]
#[non_exhaustive]
pub enum Command {
	/// Resets the link
	Reset,
}
