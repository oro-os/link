use std::{collections::HashMap, net::SocketAddr};

use anyhow::{Context, Result};
use tokio::{
	net::TcpStream,
	sync::mpsc::{Receiver, Sender},
};

use crate::{config::LinkConfig, mdns::LinkInfo};

pub async fn handle_link_connections(
	mut receiver: Receiver<LinkInfo>,
	socket_sender: Sender<(TcpStream, SocketAddr)>,
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

		let Some(_link_config) = links.get(&link_info.name) else {
			log::warn!(
				"discovered link '{}' is not in the config, skipping",
				link_info.name
			);
			continue;
		};

		log::debug!("connecting to link: {link_info:?}");

		let sock_addr: SocketAddr = (link_info.address, link_info.port).into();
		let stream = match TcpStream::connect(sock_addr).await {
			Ok(stream) => stream,
			Err(err) => {
				log::error!("failed to connect to link '{}': {:?}", link_info.name, err);
				continue;
			}
		};

		log::info!("connected to link '{}'", link_info.name);
		if let Err(err) = socket_sender.send((stream, sock_addr)).await {
			anyhow::bail!("failed to send link connection to MQTT server: {err:?}");
		}
	}
}
