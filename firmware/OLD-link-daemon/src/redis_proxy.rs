use anyhow::{Context, Result};
use tokio::{
	io::copy_bidirectional,
	net::{TcpStream, UnixStream},
};

use crate::area_controller_tcp_listener::{
	AreaControllerListenerConfig, AreaControllerStream, AreaControllerTcpListener,
};

#[derive(Clone)]
pub struct Config {
	pub listen_addr: String,
	pub listen_port: u16,
	pub area_controller_listener_config: AreaControllerListenerConfig,
	pub redis_connection_info: redis::ConnectionInfo,
}

pub async fn run(config: Config) -> Result<!> {
	let listener = AreaControllerTcpListener::bind(
		config.area_controller_listener_config.clone(),
		(config.listen_addr.as_str(), config.listen_port),
	)
	.await?;
	let local_addr = listener.local_addr()?;

	log::info!(
		"listening for area controllers on {} and proxying to {}",
		local_addr,
		config.redis_connection_info.addr(),
	);

	loop {
		let (stream, peer) = listener.accept().await?;

		log::info!("accepted connection from area controller {peer}");
		let redis_info = config.redis_connection_info.addr().clone();
		tokio::spawn(async move {
			if let Err(err) = proxy_connection(peer, stream, redis_info).await {
				log::warn!("proxy session for {peer} terminated with error: {err:#}");
			}
		});
	}
}

async fn proxy_connection(
	peer: std::net::SocketAddr,
	mut stream: AreaControllerStream,
	redis_info: redis::ConnectionAddr,
) -> Result<()> {
	match redis_info {
		redis::ConnectionAddr::Tcp(ref host, port) => {
			let mut upstream = TcpStream::connect((host.as_str(), port))
				.await
				.with_context(|| format!("failed to connect to upstream redis {redis_info:?}"))?;

			let _ = copy_bidirectional(&mut stream, &mut upstream)
				.await
				.context("proxy stream failed")?;
		}
		redis::ConnectionAddr::TcpTls { .. } => {
			anyhow::bail!("TLS configuration is not supported");
		}
		redis::ConnectionAddr::Unix(ref path) => {
			let mut upstream = UnixStream::connect(path)
				.await
				.with_context(|| format!("failed to connect to upstream redis {redis_info:?}"))?;
			let _ = copy_bidirectional(&mut stream, &mut upstream)
				.await
				.context("proxy stream failed")?;
		}
		ty => {
			anyhow::bail!("unknown redis connection type: {ty:?}");
		}
	}
	log::info!("closed proxied redis session for {peer}");
	Ok(())
}
