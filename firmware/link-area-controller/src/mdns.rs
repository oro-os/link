use std::net::Ipv4Addr;

use anyhow::{Context, Result};
use mdns_sd::ScopedIp;
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub struct LinkInfo {
	pub name:    String,
	pub address: Ipv4Addr,
	pub port:    u16,
}

pub async fn listen_for_links(sender: Sender<LinkInfo>) -> Result<()> {
	let mdns = mdns_sd::ServiceDaemon::new().context("failed to create mDNS service daemon")?;

	let browser = mdns
		.browse("_orolink._tcp.local.")
		.context("failed to browse for '_orolink._tcp.local.'")?;

	loop {
		let mdns_sd::ServiceEvent::ServiceResolved(info) = browser
			.recv_async()
			.await
			.context("failed to receive mDNS event")?
		else {
			continue;
		};

		let canonical_name = info.get_fullname();
		let Some((name, svc_name)) = canonical_name.split_once('.') else {
			log::warn!(
				"skipping mDNS service with invalid fullname '{}'",
				canonical_name
			);
			continue;
		};

		if svc_name != "_orolink._tcp.local." {
			log::warn!(
				"skipping mDNS service with unexpected service name '{}'",
				svc_name
			);
			continue;
		}

		log::info!(
			"discovered link: {} at {}:{}",
			name,
			info.get_addresses()
				.iter()
				.map(|address| address.to_string())
				.collect::<Vec<_>>()
				.join(", "),
			info.get_port()
		);

		let Some(address) = info.get_addresses().iter().find_map(|address| {
			match address {
				ScopedIp::V4(address) => Some(address),
				_ => None,
			}
		}) else {
			log::warn!("skipping link '{}' because it has no IPv4 address", name);
			continue;
		};

		sender
			.send(LinkInfo {
				name:    name.to_string(),
				address: *address.addr(),
				port:    info.get_port(),
			})
			.await
			.context("failed to send link info to main loop")?;
	}
}
