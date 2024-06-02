#![feature(never_type, async_closure)]

mod docker;
mod session;

use self::docker::Docker;
use async_std::{io, net::TcpListener, prelude::*, task};
use envconfig::Envconfig;

use link_protocol::{channel::RWError, Error as ProtoError};
use log::{debug, error, info, warn};

use std::str::FromStr;

#[derive(Envconfig, Clone)]
pub(crate) struct Config {
	#[envconfig(from = "LINK_SERVER_PORT", default = "1337")]
	pub link_server_port: u16,
	#[envconfig(from = "LINK_SERVER_BIND", default = "0.0.0.0")]
	pub link_server_bind: String,
	#[envconfig(from = "USE_JOURNALD", default = "0")]
	#[allow(unused)]
	pub use_journald: u8,
	#[envconfig(from = "DOCKER_HOST")]
	pub docker_host: String,
	#[envconfig(from = "DOCKER_REF")]
	pub docker_ref: String,
	#[envconfig(from = "GH_ACCESS_TOKEN")]
	pub gh_access_token: String,
	#[envconfig(from = "GH_ORGANIZATION")]
	pub gh_organization: String,
	#[envconfig(from = "LEVEL", default = "trace")]
	pub log_level: String,
	#[envconfig(from = "VERBOSE", default = "0")]
	pub verbose: u8,
}

#[derive(thiserror::Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum Error {
	#[error("i/o error")]
	AsyncIo(#[from] io::Error),
	#[error("i/o error during protocol transcoding")]
	Proto(#[from] ProtoError<io::Error>),
	#[error("i/o error during link connection negotiation")]
	RWError(#[from] RWError<ProtoError<io::Error>, ProtoError<io::Error>>),
	#[error("expected link to send LinkOnline but another packet was sent instead")]
	NoHelloPacket,
	#[error("unexpected packet was sent by peer (either link or client connection)")]
	UnexpectedPacket,
	#[error("docker request failed: {0}")]
	Docker(#[from] docker::Error),
	#[error("failed to receive channel message")]
	ChannelRecv,
	#[error("failed to send channel message")]
	ChannelSend,
}

impl From<async_std::channel::RecvError> for Error {
	#[inline]
	fn from(_: async_std::channel::RecvError) -> Self {
		Self::ChannelRecv
	}
}

impl<T> From<async_std::channel::SendError<T>> for Error {
	#[inline]
	fn from(_: async_std::channel::SendError<T>) -> Self {
		Self::ChannelSend
	}
}

#[async_std::main]
async fn main() -> Result<!, Error> {
	let config = Config::init_from_env().unwrap();

	let log_level = log::LevelFilter::from_str(&config.log_level)
		.expect("failed to parse LEVEL environment variable");

	log::set_max_level(log_level);

	#[cfg(all(not(feature = "journald"), not(feature = "stderr")))]
	compile_error!("one of 'journald' and/or 'stderr' must be specified as features");

	#[allow(unused)]
	let should_fallback = true;

	#[cfg(feature = "journald")]
	let should_fallback = {
		if config.use_journald != 0 {
			systemd_journal_logger::JournalLog::default()
				.with_extra_fields(vec![("VERSION", env!("CARGO_PKG_VERSION"))])
				.with_syslog_identifier("oro-linkd".to_string())
				.install()
				.expect("failed to start journald logger");
			false
		} else {
			true
		}
	};

	#[cfg(feature = "stderr")]
	if should_fallback {
		let mut slog = stderrlog::new();

		if config.verbose == 0 {
			slog.module(module_path!());
		}

		slog.show_module_names(true)
			.verbosity(log_level)
			.timestamp(stderrlog::Timestamp::Millisecond)
			.init()
			.expect("failed to start stderr logger");
	}

	info!("starting oro-linkd version {}", env!("CARGO_PKG_VERSION"));

	let docker = Docker::new(&config.docker_host).expect("failed to parse DOCKER_HOST uri");

	debug!(
		"checking to see if docker ref exists: {}",
		config.docker_ref
	);
	docker
		.check_image(&config.docker_ref)
		.await
		.unwrap_or_else(|err| panic!("failed to check image: {:?}: {}", err, config.docker_ref));

	let listener =
		TcpListener::bind((config.link_server_bind.as_str(), config.link_server_port)).await?;
	let mut incoming = listener.incoming();

	info!(
		"listening for link connections on {}:{}",
		config.link_server_bind, config.link_server_port
	);

	while let Some(stream) = incoming.next().await {
		let stream = stream?;
		let config = config.clone();

		task::spawn(async move {
			if let Err(err) = self::session::run_session(config, stream).await {
				error!("oro link peer connection encountered error: {:?}", err);
			} else {
				warn!("oro link peer connection ended with OK result");
			}
		});
	}

	async_io::Timer::never().await;
	unreachable!();
}
