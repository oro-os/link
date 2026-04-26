use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use clap::Parser;

mod area_controller_tcp_listener;
mod config;
mod link_id;
mod link_service;

#[derive(Debug, Parser)]
struct Opts {
	/// The configuration file to use.
	#[clap(short = 'c', long, default_value = "/etc/oro/link-daemon.toml")]
	config:           PathBuf,
	/// Address to bind for area-controller TLS connections.
	#[clap(long, default_value = "0.0.0.0")]
	listen_addr:      String,
	/// Port to bind for area-controller proxied link traffic.
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
}

async fn pmain() -> Result<()> {
	let opts = Opts::parse();
	let config_path = opts.config.display().to_string();

	log::info!("loading config from '{config_path}'");
	let config_str = tokio::fs::read_to_string(&opts.config)
		.await
		.with_context(|| format!("failed to read config file '{config_path}'"))?;
	let config = {
		let mut config = toml::from_str::<config::Config>(&config_str)
			.with_context(|| format!("failed to parse config file '{config_path}'"))?;
		config
			.normalize()
			.with_context(|| format!("failed to normalize config file '{config_path}'"))?;
		config
	};

	let area_controller_listener_config =
		area_controller_tcp_listener::AreaControllerListenerConfig::build(
			&opts.allowed_keys_dir,
			&opts.tls_cert,
			&opts.tls_key,
		)?;

	let service_config = link_service::Config {
		listen_addr: opts.listen_addr,
		listen_port: opts.listen_port,
		area_controller_listener_config,
		links: Arc::new(config.link),
	};

	log::debug!("entering main loop");
	tokio::select! {
		res = tokio::signal::ctrl_c() => {
			res.context("failed to listen for ctrl-c")?;
			log::info!("received ctrl-c, shutting down link-daemon");
		}
		res = link_service::run(service_config) => {
			res.context("link service failed")?;
		}
	}

	Ok(())
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
