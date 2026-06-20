#![feature(never_type)]

use anyhow::{Context, Result};
use clap::Parser;

#[derive(Debug, Parser)]
struct Args {
	/// The path to the area controller's public key.
	#[arg(long = "pub")]
	pubkey:        String,
	/// The path to the area controller's private key.
	#[arg(long = "priv")]
	privkey:       String,
	/// The path to the daemon's public key.
	#[arg(long = "daemon")]
	daemon_pubkey: String,
}

async fn pmain() -> Result<()> {
	log::info!("Hello, world!");
	run_mdns_service().await?;
}

#[tokio::main]
async fn main() {
	env_logger::builder()
		.filter_level(log::LevelFilter::Debug)
		.parse_default_env()
		.init();

	if let Err(e) = pmain().await {
		log::error!("fatal error: {e}");
		std::process::exit(1);
	}

	log::warn!("oro link area controller is exiting gracefully");
}

async fn run_mdns_service() -> Result<!> {
	let mdns = mdns_sd::ServiceDaemon::new().context("failed to create mDNS service daemon")?;

	let receiver = mdns
		.monitor()
		.context("failed to create mDNS service monitor")?;

	let service = mdns_sd::ServiceInfo::new::<&str, &[(&str, &str)]>(
		"_oro-link-aread._tcp.local.",
		"DBG_INSTANCE",
		"192.168.50.198.local.",
		"192.168.50.198",
		7778,
		&([])[..],
	)
	.context("failed to create mDNS service info")?;

	mdns.register(service)
		.context("failed to register mDNS service")?;

	loop {
		let event = receiver
			.recv_async()
			.await
			.context("failed to receive mDNS event")?;
		log::debug!("mDNS event: {event:?}");
	}
}
