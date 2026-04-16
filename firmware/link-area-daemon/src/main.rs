#![feature(never_type)]
#![feature(iter_intersperse)]

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use tokio::sync::Semaphore;

pub mod config;
pub mod daemon_config_service;
pub mod daemon_connection;
pub mod link_connection;
pub mod mdns;

/// Runs the Oro Link area control for the local network.
#[derive(Debug, Parser)]
struct Opts {
	/// The configuration file to use.
	#[clap(short = 'c', long, default_value = "/etc/oro/link-area.toml")]
	config: String,
}

async fn pmain() -> Result<()> {
	let opts = Opts::parse();

	log::info!("loading config from '{}'", opts.config);
	let config_str = tokio::fs::read_to_string(&opts.config)
		.await
		.with_context(|| format!("failed to read config file '{}'", opts.config))?;
	let config = {
		let mut config = toml::from_str::<config::Config>(&config_str)
			.with_context(|| format!("failed to parse config file '{}'", opts.config))?;
		config
			.normalize()
			.with_context(|| format!("failed to normalize config file '{}'", opts.config))?;
		config
	};
	let daemon_connection_config = daemon_connection::DaemonConnectionConfig::build(&config.daemon)
		.context("failed to build daemon TLS connection config")?;
	let link_connection_daemon_connection_config = daemon_connection_config.clone();
	let daemon_port = config.daemon.port;

	let (link_discovery_sender, link_discovery_receiver) = tokio::sync::mpsc::channel(16);

	let config_ready = Arc::new(Semaphore::new(0));

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
			&config.link,
			link_connection_daemon_connection_config,
			daemon_port,
			Arc::clone(&config_ready),
		) => {
			res.context("link connection handler failed")?;
		}
		res = daemon_config_service::run(
			&config.link,
			daemon_connection_config,
			daemon_port,
			Arc::clone(&config_ready)
		) => {
			res.context("daemon config service failed")?;
		}
	}

	log::warn!("one or more services have stopped unexpectedly, shutting down");
	anyhow::bail!("service stopped");
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

	if let Err(e) = pmain().await {
		log::error!("fatal error: {e:#}");
		std::process::exit(1);
	}

	Ok(())
}
