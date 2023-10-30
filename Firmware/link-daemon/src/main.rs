#![feature(never_type, async_fn_in_trait)]

use async_std::{
	fs, io,
	net::{TcpListener, TcpStream},
	os::unix::net::UnixListener,
	prelude::*,
	sync::{Arc, Mutex},
	task,
};
use envconfig::Envconfig;
use link_protocol::{
	channel::{negotiate, RWError, Side as ChannelSide},
	Error as ProtoError, Packet,
};
use log::{debug, error, info, trace, warn};
use rand::rngs::OsRng;
use rs_docker::Docker;
use std::path::PathBuf;

pub(crate) const IMAGE_TAG: &str = "github.com/oro-os/github-actions-runner:latest";

#[derive(Envconfig)]
struct Config {
	#[envconfig(from = "LINK_SERVER_PORT", default = "1337")]
	pub link_server_port: u16,
	#[envconfig(from = "LINK_SERVER_BIND", default = "0.0.0.0")]
	pub link_server_bind: String,
	#[envconfig(from = "USE_JOURNALD", default = "0")]
	#[allow(unused)]
	pub use_journald: u8,
	#[envconfig(from = "GA_TARBALL")]
	pub actions_tarball: String,
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
}

async fn task_process_oro_link(stream: TcpStream, docker: Arc<Mutex<Docker>>) -> Result<(), Error> {
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
	docker: Arc<Mutex<Docker>>,
) -> Result<(), Error> {
	let listener = TcpListener::bind((bind_host.as_str(), port)).await?;
	let mut incoming = listener.incoming();

	info!("listening for link connections on {}:{}", bind_host, port);

	while let Some(stream) = incoming.next().await {
		let stream = stream?;
		let docker = docker.clone();
		task::spawn(async move {
			if let Err(err) = task_process_oro_link(stream, docker).await {
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

	trace!("connecting to docker with default settings");
	let mut docker =
		Docker::connect("unix:///var/run/docker.sock").expect("failed to connect to docker");

	trace!("loading actions work image tarball");
	let tarball_data = fs::read(config.actions_tarball)
		.await
		.expect("failed to read github actions worker tarball");

	trace!("building github actions image");
	docker
		.build_image(tarball_data, IMAGE_TAG.into())
		.expect("failed to build oro github actions runner image");

	debug!("successfully built oro github actions runner image");

	let docker = Arc::new(Mutex::new(docker));

	task::spawn(async move {
		if let Err(err) =
			task_accept_oro_link_tcp(config.link_server_bind, config.link_server_port, docker).await
		{
			error!("oro link tcp server error: {:?}", err);
		}
		warn!("oro link tcp server has shut down; terminating...");
		std::process::exit(2);
	});

	async_io::Timer::never().await;
	unreachable!();
}
