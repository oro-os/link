use std::collections::HashMap;

use anyhow::{Context, Result};
use tokio::{io::copy_bidirectional, net::TcpStream, sync::mpsc::Receiver, task::JoinHandle};

use crate::{daemon_connection::DaemonConnectionConfig, mdns::LinkInfo};

async fn proxy_link_to_daemon(
	link_name: String,
	link_info: LinkInfo,
	daemon_connection_config: DaemonConnectionConfig,
) -> Result<()> {
	let mut link_stream = TcpStream::connect((link_info.address, link_info.port))
		.await
		.with_context(|| {
			format!(
				"failed to connect to link '{}' at {}:{}",
				link_name, link_info.address, link_info.port
			)
		})?;

	log::info!(
		"connected to link '{}' at {}:{}",
		link_name,
		link_info.address,
		link_info.port
	);

	let daemon_addr = daemon_connection_config.address();
	let mut daemon_tls = daemon_connection_config.connect().await?;
	log::info!("connected to daemon '{daemon_addr}' for link '{link_name}'");

	let (from_link, from_daemon) = copy_bidirectional(&mut link_stream, &mut daemon_tls)
		.await
		.with_context(|| format!("proxy stream for link '{link_name}' failed"))?;

	log::info!(
		"closed proxied link stream for '{}': {} bytes link->daemon, {} bytes daemon->link",
		link_name,
		from_link,
		from_daemon
	);

	Ok(())
}

pub async fn handle_link_connections(
	mut receiver: Receiver<LinkInfo>,
	daemon_connection_config: DaemonConnectionConfig,
) -> Result<()> {
	let mut active_connections: HashMap<String, JoinHandle<()>> = HashMap::new();

	loop {
		let link_info = receiver
			.recv()
			.await
			.context("failed to receive link info from mDNS listener")?;

		let name = link_info.name.trim().to_ascii_uppercase();
		log::info!(
			"discovered link: {} at {}:{}",
			name,
			link_info.address,
			link_info.port
		);

		if let Some(handle) = active_connections.remove(&name) {
			if !handle.is_finished() {
				log::debug!(
					"link '{}' got a refreshed mDNS resolution, aborting stale daemon proxy",
					name
				);
				handle.abort();
			}

			let _ = handle.await;
		}

		log::debug!("starting proxied daemon session for link: {link_info:?}");

		let daemon_connection_config = daemon_connection_config.clone();
		let link_name = name.clone();

		let handle = tokio::spawn(async move {
			if let Err(err) =
				proxy_link_to_daemon(link_name.clone(), link_info, daemon_connection_config).await
			{
				log::warn!(
					"link '{}' proxy session ended with error: {err:#}",
					link_name
				);
			}
		});

		active_connections.insert(name, handle);
	}
}
