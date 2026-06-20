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

	let mut counter = 0u32;
	loop {
		counter = counter.wrapping_add(1);
		client
			.set("debug:counter", format_args!("count={counter}"))
			.await
			.expect("failed to set COUNTER in redis");
		let message: Option<heapless::String<64>> = client
			.get("debug:message")
			.await
			.expect("failed to get MESSAGE from redis");
		if let Some(message) = message {
			defmt::info!("got message from redis: {}", message);
		} else {
			defmt::debug!("no message in redis");
		}
		let count: Option<u32> = client
			.get("debug:server-count")
			.await
			.expect("failed to get COUNT from redis");
		if let Some(count) = count {
			defmt::info!("got count from redis: {}", count);
		} else {
			defmt::debug!("no count in redis");
		}

		Timer::after_secs(1).await;
	}
}
