use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::{Duration, Timer};

pub type Channel<T, const N: usize = 16> = embassy_sync::channel::Channel<NoopRawMutex, T, N>;
pub type Sender<T, const N: usize = 16> =
	embassy_sync::channel::Sender<'static, NoopRawMutex, T, N>;
pub type Receiver<T, const N: usize = 16> =
	embassy_sync::channel::Receiver<'static, NoopRawMutex, T, N>;

pub trait ReceiveDelay<T> {
	async fn after_receive(&self, duration: Duration) -> Result<(), T>;
}

impl<M, T, const N: usize> ReceiveDelay<T> for embassy_sync::channel::Channel<M, T, N>
where
	M: embassy_sync::blocking_mutex::raw::RawMutex,
{
	async fn after_receive(&self, duration: Duration) -> Result<(), T> {
		let r = embassy_futures::select::select(Timer::after(duration), self.receive()).await;

		match r {
			embassy_futures::select::Either::First(_) => Ok(()),
			embassy_futures::select::Either::Second(msg) => Err(msg),
		}
	}
}

pub trait ChannelExt {
	type Channel;
	type Receiver;
	type Sender;
}

impl<T: 'static, const N: usize> ChannelExt for Channel<T, N> {
	type Channel = Self;
	type Receiver = Receiver<T, N>;
	type Sender = Sender<T, N>;
}

impl<T: 'static, const N: usize> ChannelExt for Receiver<T, N> {
	type Channel = Channel<T, N>;
	type Receiver = Self;
	type Sender = Sender<T, N>;
}

impl<T: 'static, const N: usize> ChannelExt for Sender<T, N> {
	type Channel = Channel<T, N>;
	type Receiver = Receiver<T, N>;
	type Sender = Self;
}
