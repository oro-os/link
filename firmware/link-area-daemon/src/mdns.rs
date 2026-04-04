use anyhow::{Context, Result};

pub async fn listen_for_links() -> Result<!> {
	let mdns = mdns_sd::ServiceDaemon::new().context("failed to create mDNS service daemon")?;

	let browser = mdns
		.browse("_orolink._tcp.local.")
		.context("failed to browse for '_orolink._tcp.local.'")?;

	loop {
		let ev = browser
			.recv_async()
			.await
			.context("failed to receive mDNS event")?;
		match ev {
			mdns_sd::ServiceEvent::ServiceResolved(info) => {
				log::info!(
					"discovered link: {} at {}:{}",
					info.get_fullname(),
					info.get_addresses()
						.iter()
						.map(|a| a.to_string())
						.collect::<Vec<_>>()
						.join(", "),
					info.get_port()
				);
			}
			other => {
				log::debug!("mDNS event: {:?}", other);
			}
		}
	}
}
