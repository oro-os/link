use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

mod area_controller_tcp_listener;
mod mqtt_proxy;

#[derive(Debug, Parser)]
struct Opts {
	/// Address to bind for area-controller TLS connections.
	#[clap(long, default_value = "0.0.0.0")]
	listen_addr:      PathOrHost,
	/// Port to bind for area-controller MQTT-over-TLS proxy traffic.
	#[clap(long, default_value_t = 5544)]
	listen_port:      u16,
	/// Directory containing allowed client public keys or certificates.
	#[clap(long, default_value = "/etc/oro/linkd/allowed/")]
	allowed_keys_dir: PathBuf,
	/// TLS certificate chain to present to area controllers.
	#[clap(long)]
	tls_cert:         PathBuf,
	/// TLS private key matching `--tls-cert`.
	#[clap(long)]
	tls_key:          PathBuf,
	/// Hostname or IP of the upstream MQTT broker.
	#[clap(long, default_value = "127.0.0.1")]
	mqtt_host:        String,
	/// TCP port of the upstream MQTT broker.
	#[clap(long, default_value_t = 1883)]
	mqtt_port:        u16,
}

#[derive(Clone, Debug)]
struct PathOrHost(String);

impl std::str::FromStr for PathOrHost {
	type Err = std::convert::Infallible;

	fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
		Ok(Self(value.to_string()))
	}
}

async fn pmain() -> Result<()> {
	let opts = Opts::parse();
	let area_controller_listener_config =
		area_controller_tcp_listener::AreaControllerListenerConfig::build(
			&opts.allowed_keys_dir,
			&opts.tls_cert,
			&opts.tls_key,
		)?;
	let proxy_config = mqtt_proxy::Config {
		listen_addr: opts.listen_addr.0,
		listen_port: opts.listen_port,
		area_controller_listener_config,
		mqtt_host: opts.mqtt_host,
		mqtt_port: opts.mqtt_port,
	};

	log::debug!("entering main loop");
	tokio::select! {
		res = tokio::signal::ctrl_c() => {
			res.context("failed to listen for ctrl-c")?;
			log::info!("received ctrl-c, shutting down link-daemon");
			return Ok(());
		}
		res = mqtt_proxy::run(proxy_config) => {
			res.context("MQTT proxy task failed")?;
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

	log::info!("starting Oro Link daemon v{}", env!("CARGO_PKG_VERSION"));

	if let Err(err) = pmain().await {
		log::error!("fatal error: {err:#}");
		std::process::exit(1);
	}

	Ok(())
}
