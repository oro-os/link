pub use embassy_net_enc28j60::Enc28j60;
use embedded_hal::{digital::OutputPin, spi::SpiDevice};

const MAX_ETH_FRAME: usize = 1514;

impl<S, O> crate::uc::RawEthernetDriver for Enc28j60<S, O>
where
	S: SpiDevice,
	O: OutputPin,
{
	#[inline]
	fn address(&self) -> [u8; 6] {
		Enc28j60::address(self)
	}

	#[inline]
	async fn try_recv(&mut self, buf: &mut [u8]) -> Option<usize> {
		assert!(
			buf.len() >= MAX_ETH_FRAME,
			"`buf` is smaller than the maximum ethernet frame"
		);
		Enc28j60::receive(self, buf)
	}

	#[inline]
	async fn send(&mut self, buf: &[u8]) {
		// We don't need to panic here since the enc28j60 library does it for us.
		Enc28j60::transmit(self, buf)
	}

	#[inline]
	fn is_link_up(&mut self) -> bool {
		Enc28j60::is_link_up(self)
	}
}
