//! Redis service implementation for the Link firmware.

use embassy_net::{IpEndpoint, Stack};
use embassy_time::Timer;
use static_cell::StaticCell;

pub type Channel = crate::channel::Channel<Cmd, 4>;

pub enum Cmd {
	Connect { endpoint: IpEndpoint },
}

pub struct Config {
	pub stack: Stack<'static>,
}

#[embassy_executor::task]
#[allow(irrefutable_let_patterns)]
pub async fn run(rx: &'static Channel, config: Config) -> ! {
	// Wait for service endpoint
	let endpoint = loop {
		if let Cmd::Connect { endpoint } = rx.receive().await {
			break endpoint;
		} else {
			defmt::warn!("unexpected command received before Connect");
		}
	};

	static RX: StaticCell<[u8; 4096]> = StaticCell::new();
	static TX: StaticCell<[u8; 4096]> = StaticCell::new();
	static CMD_BUF: StaticCell<heapless::Vec<u8, 512>> = StaticCell::new();

	let rx = RX.init([0; 4096]);
	let tx = TX.init([0; 4096]);
	let cmd_buf = CMD_BUF.init(heapless::Vec::new());

	let mut sock = embassy_net::tcp::TcpSocket::new(config.stack, rx.as_mut(), tx.as_mut());
	defmt::info!(
		"connecting to redis (via area controller) at {:?})...",
		endpoint
	);
	sock.connect(endpoint)
		.await
		.expect("failed to connect to redis");
	defmt::info!("connected to redis at {:?}; pinging...", endpoint);

	let prefix = heapless::format!(64; "link:{}:", crate::unique_id()).unwrap();

	let mut client = crate::redis::Client::new(sock, cmd_buf, &prefix);
	client.ping().await.expect("failed to ping redis");
	defmt::info!("pinged redis successfully");

	loop {
		crate::vars::foreach_var!(var => {
			sync_var(var, &mut client).await;
		});

		embassy_futures::select::select(Timer::after_secs(3), crate::vars::DIRTY_FLAG.wait()).await;
	}
}

async fn sync_var(var: &impl crate::vars::SyncVar, client: &mut crate::redis::Client<'_, 512>) {
	use crate::redis::Error;
	if let Err(err) = var.sync(client).await {
		match err {
			Error::ProtocolError => {
				defmt::error!("protocol error while syncing variable; resetting");
			}
			Error::ReadExact(err) => {
				defmt::error!("read error while syncing variable: {:?}", err);
			}
			Error::Tcp(err) => {
				defmt::error!("TCP error while syncing variable: {:?}", err);
			}
			Error::UnexpectedResponse => {
				defmt::warn!("unexpected response while syncing variable");
				// We don't reset here; we just don't update it.
				return;
			}
			Error::TooLong => {
				defmt::error!("value too long while syncing variable");
				// We don't reset here; we just don't update it.
				return;
			}
		}
		// SAFETY: No other choice but to reset.
		unsafe {
			crate::reset();
		};
	}
}
