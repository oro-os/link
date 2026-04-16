use anyhow::{Context, Result};
use tokio::{io::copy_bidirectional, net::TcpStream};

use crate::area_controller_tcp_listener::{
	AreaControllerListenerConfig, AreaControllerStream, AreaControllerTcpListener,
};

#[derive(Clone)]
pub struct Config {
	pub listen_addr: String,
	pub listen_port: u16,
	pub area_controller_listener_config: AreaControllerListenerConfig,
	pub mqtt_host: String,
	pub mqtt_port: u16,
}

pub async fn run(config: Config) -> Result<!> {
	let listener = AreaControllerTcpListener::bind(
		config.area_controller_listener_config.clone(),
		(config.listen_addr.as_str(), config.listen_port),
	)
	.await?;
	let local_addr = listener.local_addr()?;

	log::info!(
		"listening for area controllers on {} and proxying to {}:{}",
		local_addr,
		config.mqtt_host,
		config.mqtt_port
	);

	loop {
		let (stream, peer) = listener.accept().await?;
		let mqtt_host = config.mqtt_host.clone();
		let mqtt_port = config.mqtt_port;

		log::info!("accepted connection from area controller {peer}");
		tokio::spawn(async move {
			if let Err(err) = proxy_connection(peer, stream, mqtt_host, mqtt_port).await {
				log::warn!("proxy session for {peer} terminated with error: {err:#}");
			}
		});
	}
}

async fn proxy_connection(
	peer: std::net::SocketAddr,
	mut stream: AreaControllerStream,
	mqtt_host: String,
	mqtt_port: u16,
) -> Result<()> {
	let mut upstream = TcpStream::connect((mqtt_host.as_str(), mqtt_port))
		.await
		.with_context(|| format!("failed to connect to upstream MQTT {mqtt_host}:{mqtt_port}"))?;

	let (from_area, from_broker) = copy_bidirectional(&mut stream, &mut upstream)
		.await
		.context("proxy stream failed")?;

	log::info!(
		"closed proxied MQTT session for {peer}: {from_area} bytes area->broker, {from_broker} \
		 bytes broker->area"
	);

	Ok(())
}
