#![feature(never_type)]

use std::net::SocketAddr;

use anyhow::{Context, Result};
use clap::Parser;
use rmqtt::{context::ServerContext, net::Builder as MqttBuilder, server::MqttServer};

pub mod config;
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

	let scx = ServerContext::new().build().await;

	let sockaddr = SocketAddr::new(
		config
			.instance
			.bind
			.parse()
			.context("failed to parse bind address")?,
		config.instance.port,
	);

	let mqtt = MqttServer::new(scx)
		.listener(
			MqttBuilder::new()
				.name("internal/tcp")
				.laddr(sockaddr)
				.bind()?
				.tcp()?,
		)
		.build();

	log::info!(
		"listening on {}:{}",
		config.instance.bind,
		config.instance.port
	);

	let (link_discovery_sender, link_discovery_receiver) = tokio::sync::mpsc::channel(16);

	log::debug!("entering main loop");
	tokio::select! {
		res = mqtt.run() => {
			res.context("MQTT server failed")?;
			log::warn!("MQTT server stopped unexpectedly");
		}
		res = mdns::listen_for_links(link_discovery_sender) => {
			res.context("mDNS listener failed")?;
		}
		res = link_connection::handle_link_connections(link_discovery_receiver, &config.link) => {
			res.context("link connection handler failed")?;
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
