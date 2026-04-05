use std::collections::HashMap;

use anyhow::{Context, Result};
use tokio::{net::TcpStream, sync::mpsc::Receiver};

use crate::{config::LinkConfig, mdns::LinkInfo};

pub async fn handle_link_connections(
	mut receiver: Receiver<LinkInfo>,
	links: &HashMap<String, LinkConfig>,
) -> Result<!> {
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

		let Some(link_config) = links.get(&link_info.name) else {
			log::warn!(
				"discovered link '{}' is not in the config, skipping",
				link_info.name
			);
			continue;
		};

		log::debug!("connecting to link: {link_info:?}");

		let stream = match TcpStream::connect((link_info.address, link_info.port)).await {
			Ok(stream) => stream,
			Err(err) => {
				log::error!("failed to connect to link '{}': {:?}", link_info.name, err);
				continue;
			}
		};

		log::info!("connected to link '{}'", link_info.name);

		log::debug!("handling link connections (TODO)");
		tokio::time::sleep(std::time::Duration::from_secs(60)).await;
	}
}
