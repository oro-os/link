use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

mod daemon_connection;
mod link_connection;
mod mdns;

#[derive(Debug, Parser)]
struct Opts {
	/// Hostname of the centralized daemon.
	#[clap(long, default_value = "ci.oro.sh")]
	daemon_host:       String,
	/// TLS port exposed by the centralized daemon.
	#[clap(long, default_value_t = 5544)]
	daemon_port:       u16,
	/// PEM public key or certificate used to pin the daemon's TLS identity.
	#[clap(long)]
	daemon_server_key: PathBuf,
	/// Client certificate used for mutual TLS to the daemon.
	#[clap(long)]
	client_cert:       PathBuf,
	/// Client private key matching `--client-cert`.
	#[clap(long)]
	client_key:        PathBuf,
}

async fn pmain() -> Result<()> {
	let opts = Opts::parse();
	let daemon_connection_config = daemon_connection::DaemonConnectionConfig::build(
		&opts.daemon_host,
		opts.daemon_port,
		&opts.daemon_server_key,
		&opts.client_cert,
		&opts.client_key,
	)
	.context("failed to build daemon TLS connection config")?;

	let (link_discovery_sender, link_discovery_receiver) = tokio::sync::mpsc::channel(16);

	log::debug!("entering main loop");
	tokio::select! {
		res = tokio::signal::ctrl_c() => {
			res.context("failed to listen for ctrl-c")?;
			log::warn!("received ctrl-c, shutting down");
		}
		res = mdns::listen_for_links(link_discovery_sender) => {
			res.context("mDNS listener failed")?;
		}
		res = link_connection::handle_link_connections(
			link_discovery_receiver,
			daemon_connection_config,
		) => {
			res.context("link connection handler failed")?;
		}
	}

	log::warn!("one or more services have stopped unexpectedly, shutting down");
	anyhow::bail!("service stopped")
}

#[tokio::main]
async fn main() -> Result<()> {
	env_logger::builder()
		.filter(None, log::LevelFilter::Info)
		.parse_default_env()
		.init();
	log::info!(
		"starting Oro Link area controller v{}",
		env!("CARGO_PKG_VERSION")
	);

	if let Err(err) = pmain().await {
		log::error!("fatal error: {err:#}");
		std::process::exit(1);
	}

	Ok(())
}
