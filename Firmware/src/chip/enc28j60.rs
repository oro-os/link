#![allow(deprecated)]

use core::task::Context;
use embassy_net_driver::{
	Capabilities, Checksum, ChecksumCapabilities, Driver, HardwareAddress, LinkState, Medium,
	RxToken, TxToken,
};
use embassy_time::{block_for, Duration};
use embedded_hal::{
	blocking::{
		delay::DelayMs,
		spi::{Transfer, Write},
	},
	digital::{InputPin, OutputPin},
};
use enc28j60::smoltcp_phy as encphy;
use smoltcp::phy as smolphy;

struct EmbassyDelay;

impl DelayMs<u8> for EmbassyDelay {
	fn delay_ms(&mut self, ms: u8) {
		block_for(Duration::from_millis(ms as u64));
	}
}

pub struct Enc28j60<'a, SPI, NCS, INT, RESET> {
	phy: encphy::Phy<'a, SPI, NCS, INT, RESET>,
	mac_addr: [u8; 6],
}

#[allow(unused)]
impl<'a, E, SPI, NCS, INT, RESET> Enc28j60<'a, SPI, NCS, INT, RESET>
where
	SPI: Transfer<u8, Error = E> + Write<u8, Error = E>,
	NCS: OutputPin,
	INT: InputPin + 'static,
	RESET: OutputPin + 'static,
{
	pub fn new(
		spi: SPI,
		ncs: NCS,
		int: INT,
		reset: RESET,
		mac: [u8; 6],
		rx_buf: &'a mut [u8],
		tx_buf: &'a mut [u8],
	) -> Result<Self, enc28j60::Error<E>> {
		Ok(Self {
			phy: encphy::Phy::new(
				enc28j60::Enc28j60::new(
					spi,
					ncs,
					int,
					reset,
					&mut EmbassyDelay,
					0x1000,
					mac.clone(),
				)?,
				rx_buf,
				tx_buf,
			),
			mac_addr: mac,
		})
	}
}

pub struct ProxyTxToken<'a, SPI, NCS, INT, RESET>(encphy::TxToken<'a, SPI, NCS, INT, RESET>);
pub struct ProxyRxToken<'a>(encphy::RxToken<'a>);

impl<E, SPI, NCS, INT, RESET> TxToken for ProxyTxToken<'_, SPI, NCS, INT, RESET>
where
	SPI: Transfer<u8, Error = E> + Write<u8, Error = E>,
	NCS: OutputPin,
	INT: InputPin + 'static,
	RESET: OutputPin + 'static,
{
	#[inline(always)]
	fn consume<R, F>(self, len: usize, f: F) -> R
	where
		F: FnOnce(&mut [u8]) -> R,
	{
		use smoltcp::phy::TxToken;
		self.0.consume(len, f)
	}
}

impl RxToken for ProxyRxToken<'_> {
	#[inline(always)]
	fn consume<R, F>(self, f: F) -> R
	where
		F: FnOnce(&mut [u8]) -> R,
	{
		use smoltcp::phy::RxToken;
		self.0.consume(f)
	}
}

const fn convert_checksum(sp: smolphy::Checksum) -> Checksum {
	match sp {
		smolphy::Checksum::Both => Checksum::Both,
		smolphy::Checksum::Rx => Checksum::Rx,
		smolphy::Checksum::Tx => Checksum::Tx,
		smolphy::Checksum::None => Checksum::None,
	}
}

impl<E, SPI, NCS, INT, RESET> Driver for Enc28j60<'_, SPI, NCS, INT, RESET>
where
	SPI: Transfer<u8, Error = E> + Write<u8, Error = E> + 'static,
	NCS: OutputPin + 'static,
	INT: InputPin + 'static,
	RESET: OutputPin + 'static,
{
	type RxToken<'a> = ProxyRxToken<'a> where Self: 'a;
	type TxToken<'a> = ProxyTxToken<'a, SPI, NCS, INT, RESET> where Self: 'a, SPI: 'a, NCS: 'a, INT: 'a, RESET: 'a;

	fn receive(&mut self, cx: &mut Context<'_>) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
		todo!();
	}
	fn transmit(&mut self, cx: &mut Context<'_>) -> Option<Self::TxToken<'_>> {
		todo!();
	}
	fn link_state(&mut self, cx: &mut Context<'_>) -> LinkState {
		match self.phy.is_up() {
			Ok(true) => LinkState::Up,
			Ok(false) | Err(_) => LinkState::Down
		}
	}
	fn capabilities(&self) -> Capabilities {
		// TODO maybe try to find a better way to do this?
		use smolphy::Device;
		let devcaps = self.phy.capabilities();

		let mut ckcaps = ChecksumCapabilities::default();
		ckcaps.ipv4 = convert_checksum(devcaps.checksum.ipv4);
		ckcaps.udp = convert_checksum(devcaps.checksum.udp);
		ckcaps.tcp = convert_checksum(devcaps.checksum.tcp);
		ckcaps.icmpv4 = convert_checksum(devcaps.checksum.icmpv4);
		ckcaps.icmpv6 = convert_checksum(devcaps.checksum.icmpv6);

		let mut caps = Capabilities::default();
		caps.medium = match devcaps.medium {
			smolphy::Medium::Ethernet => Medium::Ethernet,
			smolphy::Medium::Ip => Medium::Ip,
		};
		caps.max_transmission_unit = devcaps.max_transmission_unit;
		caps.max_burst_size = devcaps.max_burst_size;
		caps.checksum = ckcaps;

		caps
	}
	fn hardware_address(&self) -> HardwareAddress {
		HardwareAddress::Ethernet(self.mac_addr.clone())
	}
}
