use embassy_net::{IpEndpoint, Stack};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use heapless::String;
use static_cell::StaticCell;

pub type Channel = crate::channel::Channel<Cmd, 32>;

pub struct Config {
	pub stack:           Stack<'static>,
	pub daemon_endpoint: String<64>,
}

#[expect(dead_code, reason = "Temporary")]
pub enum Cmd {
	Noop,
}

#[embassy_executor::task]
pub async fn run(_bus: super::Bus, _rx: &'static Channel, config: Config) -> ! {
	static RX_BUF: StaticCell<[u8; 4096]> = StaticCell::new();
	static TX_BUF: StaticCell<[u8; 4096]> = StaticCell::new();

	let rx_buf = RX_BUF.init([0; 4096]);
	let tx_buf = TX_BUF.init([0; 4096]);

	loop {
		defmt::debug!("waiting for stack to come up");
		config.stack.wait_config_up().await;
		defmt::debug!("stack is up, connecting to daemon");

		let (hostname, port) = match config.daemon_endpoint.split_once(':') {
			Some((hostname, port)) => (hostname, port),
			None => {
				defmt::warn!("invalid daemon endpoint '{}'", config.daemon_endpoint);
				Timer::after(Duration::from_secs(5)).await;
				continue;
			}
		};

		let port: u16 = match port.parse() {
			Ok(port) => port,
			Err(_) => {
				// TODO: show on oled and panic
				defmt::warn!(
					"invalid port in daemon endpoint '{}'",
					config.daemon_endpoint
				);
				Timer::after(Duration::from_secs(5)).await;
				continue;
			}
		};

		defmt::debug!(
			"attempting to resolve daemon endpoint '{}'",
			config.daemon_endpoint
		);
		let addr = match crate::dns::resolve(config.stack, hostname).await {
			Ok(addr) => addr,
			Err(err) => {
				defmt::warn!(
					"failed to resolve daemon endpoint '{}': {:?}",
					config.daemon_endpoint,
					err
				);
				Timer::after(Duration::from_secs(5)).await;
				continue;
			}
		};

		let mut socket = embassy_net::tcp::TcpSocket::new(config.stack, rx_buf, tx_buf);
		defmt::debug!("attempting to connect to daemon at {}:{}", addr, port);
		match socket
			.connect(IpEndpoint::new(embassy_net::IpAddress::Ipv4(addr), port))
			.await
		{
			Ok(()) => {
				defmt::debug!("connected to daemon at {}:{}", addr, port);
			}
			Err(err) => {
				defmt::warn!(
					"failed to connect to daemon at {}:{}: {:?}",
					addr,
					port,
					err
				);
				Timer::after(Duration::from_secs(5)).await;
				continue;
			}
		}

		if let Err(err) = socket.write_all(b"hello from link-firmware!\n").await {
			defmt::warn!("failed to write to daemon at {}:{}: {:?}", addr, port, err);
			Timer::after(Duration::from_secs(5)).await;
			continue;
		}

		defmt::debug!("successfully wrote to daemon at {}:{}", addr, port);
		loop {
			Timer::after(Duration::from_secs(60)).await;
		}
	}
}
