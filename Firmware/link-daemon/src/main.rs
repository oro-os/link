#![feature(never_type, async_fn_in_trait)]

mod docker;

use self::docker::Docker;
use async_std::{
	io,
	net::{TcpListener, TcpStream},
	os::unix::net::UnixListener,
	prelude::*,
	sync::Arc,
	task,
};
use envconfig::Envconfig;
use link_protocol::{
	channel::{negotiate, RWError, Side as ChannelSide},
	Error as ProtoError, Packet,
};
use log::{debug, error, info, trace, warn};
use rand::rngs::OsRng;
use std::path::PathBuf;

#[derive(Envconfig)]
struct Config {
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
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error("i/o error")]
	AsyncIo(#[from] io::Error),
	#[error("i/o error during protocol transcoding")]
	Proto(#[from] ProtoError<io::Error>),
	#[error("i/o error during link connection negotiation")]
	RWError(#[from] RWError<ProtoError<io::Error>, ProtoError<io::Error>>),
	#[error("expected link to send LinkOnline but another packet was sent instead")]
	NoHelloPacket,
	#[error("docker request failed: {0}")]
	Docker(#[from] docker::Error),
}

async fn task_process_oro_link(
	stream: TcpStream,
	docker: Docker,
	config: Arc<Config>,
) -> Result<(), Error> {
	debug!("incoming oro link connection");

	let receiver = io::BufReader::new(stream.clone());
	let sender = io::BufWriter::new(stream);

	debug!("created streams; negotiating");
	let (mut sender, mut receiver) =
		negotiate(sender, receiver, &mut OsRng, ChannelSide::Server).await?;

	debug!("negotiated; waiting for hello");
	let link_id = if let Packet::LinkOnline { uid, version } = receiver.receive().await? {
		let hexid = ::hex::encode_upper(uid);
		info!("oro link came online");
		info!("    link firmware version: {}", version);
		info!("    link UID:              {}", hexid);
		hexid
	} else {
		warn!("link sent something other than a hello packet");
		return Err(Error::NoHelloPacket);
	};

	let id = docker
		.create_container(&docker::CreateContainer {
			image: config.docker_ref.clone(),
			labels: Some(docker::Map::new().add("sh.oro".into(), "link".into())),
			env: Some(
				docker::Args::new()
					.add("ACCESS_TOKEN".into(), config.gh_access_token.clone())
					.add("ORGANIZATION".into(), config.gh_organization.clone())
					.add("LABELS".into(), "self-hosted,oro,oro-link,x86_64".into()) // TODO(qix-): use self-report functionality of link
					.add("NAME".into(), link_id),
			),
			..Default::default()
		})
		.await?;

	debug!("created actions runner container: {}", id);

	loop {
		let packet = receiver.receive().await?;

		match packet {
			Packet::TftpRequest(pathname) => {
				trace!("TFTP requested file: {}", pathname);
				match pathname.as_str() {
					"ORO_BOOT" => {}
					filepath => {
						warn!("TFTP client requested unknown pathname: {}", filepath);
						sender
							.send(Packet::TftpError(0, "unknown pathname".into()))
							.await?; // FIXME: this is probably not the correct way to do this.
					}
				}
			}
			unknown => warn!("dropping unknown packet: {:?}", unknown),
		}
	}
}

async fn task_accept_oro_link_tcp(
	bind_host: String,
	port: u16,
	docker: Docker,
	config: Arc<Config>,
) -> Result<(), Error> {
	let listener = TcpListener::bind((bind_host.as_str(), port)).await?;
	let mut incoming = listener.incoming();

	info!("listening for link connections on {}:{}", bind_host, port);

	while let Some(stream) = incoming.next().await {
		let stream = stream?;
		let docker = docker.clone();
		let config = config.clone();
		task::spawn(async move {
			if let Err(err) = task_process_oro_link(stream, docker, config.clone()).await {
				error!("oro link peer connection encountered error: {:?}", err);
			}
		});
	}

	Ok(())
}

#[async_std::main]
async fn main() -> Result<!, Error> {
	let config = Config::init_from_env().unwrap();

	log::set_max_level(log::LevelFilter::Trace);

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
		stderrlog::new()
			//.module(module_path!()) // FIXME: whitelist?
			.show_module_names(true)
			.verbosity(log::max_level())
			.timestamp(stderrlog::Timestamp::Millisecond)
			.init()
			.expect("failed to start stderr logger");
	}

	info!("starting oro-linkd version {}", env!("CARGO_PKG_VERSION"));

	let docker = Docker::new(&config.docker_host).expect("failed to parse DOCKER_HOST uri");

	trace!("building github actions image");
	docker
		.check_image(&config.docker_ref)
		.await
		.unwrap_or_else(|err| panic!("failed to check image: {:?}: {}", err, config.docker_ref));

	debug!("successfully built oro github actions runner image");

	let config = Arc::new(config);

	task::spawn(async move {
		if let Err(err) = task_accept_oro_link_tcp(
			config.link_server_bind.clone(),
			config.link_server_port,
			docker,
			config,
		)
		.await
		{
			error!("oro link tcp server error: {:?}", err);
		}
		warn!("oro link tcp server has shut down; terminating...");
		std::process::exit(2);
	});

	async_io::Timer::never().await;
	unreachable!();
}
