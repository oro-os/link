#![feature(never_type, async_fn_in_trait, async_closure)]

mod docker;

use self::docker::Docker;
use async_std::{
	fs, io,
	net::{TcpListener, TcpStream},
	os::unix::net::{UnixListener, UnixStream},
	prelude::*,
	sync::{Arc, Mutex},
	task,
};
use envconfig::Envconfig;
use futures::{future::FutureExt, select};
use link_protocol::{
	channel::{negotiate, RWError, Side as ChannelSide},
	Error as ProtoError, Packet, PowerState, Scene,
};
use log::{debug, error, info, trace, warn};
use rand::rngs::OsRng;
use std::{os::unix::fs::PermissionsExt, str::FromStr};

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
	#[envconfig(from = "LEVEL", default = "trace")]
	pub log_level: String,
	#[envconfig(from = "VERBOSE", default = "0")]
	pub verbose: u8,
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

struct ContainerGuard<'a> {
	docker: &'a Docker,
	id: String,
}

impl<'a> Drop for ContainerGuard<'a> {
	fn drop(&mut self) {
		let docker = self.docker.clone();
		let id = self.id.clone();
		debug!("dropping container guard; killing container: {}", id);
		task::spawn(async move {
			if let Err(err) = docker.remove_container(&id, true).await {
				error!("failed to kill docker container: {:?}", err);
			}
		});
	}
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

	let uds_path = format!("/tmp/link-{link_id}.sock");
	if let Err(err) = fs::remove_file(&uds_path).await {
		if err.kind() == io::ErrorKind::NotFound {
			trace!("uds path doesn't exist: {}", uds_path);
		} else {
			error!(
				"failed to remove previous link socket: {:?}: {}",
				err, uds_path
			);
			return Err(err.into());
		}
	}

	let uds = UnixListener::bind(uds_path.to_string()).await?;
	fs::set_permissions(&uds_path, fs::Permissions::from_mode(0o777)).await?;

	debug!("pruning all containers for this link: {link_id}");
	let containers = docker
		.list_containers(Some(vec![("sh.oro.link".into(), link_id.clone())]))
		.await?;
	debug!("pruning {} containers:", containers.len());
	for (id, state) in containers {
		debug!("    - {id} ({state})");
		docker.remove_container(&id, true).await?;
	}

	let id = docker
		.create_container(&docker::CreateContainer {
			image: config.docker_ref.clone(),
			labels: Some(
				docker::Map::new()
					.add("sh.oro".into(), "link".into())
					.add("sh.oro.link".into(), link_id.clone()),
			),
			env: Some(
				docker::Args::new()
					.add("ACCESS_TOKEN".into(), config.gh_access_token.clone())
					.add("ORGANIZATION".into(), config.gh_organization.clone())
					.add("LABELS".into(), "self-hosted,oro,oro-link,x86_64".into()) // TODO(qix-): use self-report functionality of link
					.add("NAME".into(), link_id.clone()),
			),
			host_config: Some(docker::HostConfig {
				binds: Some(docker::Binds(vec![(
					uds_path,
					"/oro-link.sock".into(),
					Some("rw".into()),
				)])),
			}),
			..Default::default()
		})
		.await?;

	debug!("created actions runner container: {}", id);

	let _container_guard = ContainerGuard {
		docker: &docker,
		id: id.clone(),
	};

	let r = ({
		let docker = docker.clone();
		async || {
			docker.start_container(&id).await?;

			let container_barrier = Arc::new(Mutex::new(()));

			let container_handle = task::spawn({
				let id = id.clone();
				let barrier = container_barrier.clone();
				async move {
					let _lock = match barrier.try_lock() {
						Some(l) => l,
						None => panic!("container didn't acquire alive check mutex lock"),
					};
					docker.wait_for_container(&id).await
				}
			});

			info!("actions runner for {link_id} online and waiting for jobs");

			let mut uds_incoming = uds.incoming();

			let uds_stream = 'get_stream: loop {
				#[allow(clippy::large_enum_variant)]
				enum EventType {
					ContainerExited,
					Packet(Packet),
					ActionsConnection(UnixStream),
				}

				let event = select! {
					_ = container_barrier.lock().fuse() => EventType::ContainerExited,
					packet = receiver.receive().fuse() => EventType::Packet(packet?),
					stream = uds_incoming.next().fuse() => EventType::ActionsConnection(stream.unwrap()?),
				};

				match event {
					EventType::ActionsConnection(stream) => {
						info!(
							"actions runner for {link_id} received connection; starting test session"
						);

						break 'get_stream stream;
					}
					EventType::ContainerExited => {
						warn!(
							"github actions runner container exited unexpectedly; fetching result"
						);
						let res = container_handle.await;
						error!("github actions funner container exited: {:?}", res);
						return Ok(res?);
					}
					EventType::Packet(unknown) => warn!("dropping unknown packet from link: {:?}", unknown),
				}
			};

			let (mut uds_sender, mut uds_receiver) = negotiate(
				io::BufWriter::new(uds_stream.clone()),
				io::BufReader::new(uds_stream),
				&mut OsRng,
				ChannelSide::Server,
			).await?;

			let mut bootfile_size_packet = None;
			let mut start_test_suite_packet = None;
			let mut has_booted = false;

			loop {
				#[allow(clippy::large_enum_variant)]
				enum EventType {
					ContainerExited,
					Packet(Packet),
					UdsPacket(Packet),
				}

				let event = select! {
					_ = container_barrier.lock().fuse() => EventType::ContainerExited,
					packet = receiver.receive().fuse() => EventType::Packet(packet?),
					uds_packet = uds_receiver.receive().fuse() => EventType::UdsPacket(uds_packet?),
				};

				match event {
					EventType::ContainerExited => {
						warn!(
							"github actions runner container exited unexpectedly; fetching result"
						);
						let res = container_handle.await;
						error!("github actions funner container exited: {:?}", res);
						return Ok(res?);
					}
					EventType::UdsPacket(Packet::Serial(data)) => {
						trace!("serial: daemon -> link: {}", data.len());
						sender.send(Packet::Serial(data)).await?;
					}
					EventType::Packet(Packet::Serial(data)) => {
						trace!("serial: link -> daemon: {}", data.len());
						uds_sender.send(Packet::Serial(data)).await?;
					}
					EventType::UdsPacket(Packet::BootfileSize { uefi, bios }) => {
						trace!("bootfile size: daemon -> link: uefi={uefi} bios={bios}");
						bootfile_size_packet = Some(Packet::BootfileSize { uefi, bios });
					}
					EventType::UdsPacket(Packet::Tftp(data)) => {
						// If we've started a session, start showing it now that TFTP
						// has started.
						if let Some(packet) = start_test_suite_packet.take() {
							debug!("got first tftp packet; sending test suite start packet");
							sender.send(packet).await?;
							sender.send(Packet::SetScene(Scene::Test)).await?;
						}

						trace!("tftp: daemon -> link: {}", data.len());
						sender.send(Packet::Tftp(data)).await?;
					}
					EventType::Packet(Packet::Tftp(data)) => {
						trace!("tftp: link -> daemon: {}", data.len());
						uds_sender.send(Packet::Tftp(data)).await?;
					}
					EventType::UdsPacket(Packet::PressPower) => {
						trace!("press power: daemon -> link");
						sender.send(Packet::PressPower).await?;
					}
					EventType::UdsPacket(Packet::PressReset) => {
						trace!("press reset: daemon -> link");
						sender.send(Packet::PressReset).await?;
					}
					EventType::UdsPacket(Packet::StartTest { name }) => {
						trace!("start test: daemon -> link: {}", name);
						sender.send(Packet::StartTest { name }).await?;
					}
					EventType::UdsPacket(Packet::StartTestSession { total_tests, author, title, ref_id }) => {
						trace!("start test session: daemon -> link: total_tests={total_tests} author={author} title={title} ref_id={ref_id}");
						start_test_suite_packet = Some(Packet::StartTestSession { total_tests, author, title, ref_id });
					}
					EventType::UdsPacket(unknown) => warn!("dropping unknown packet from container: {unknown:?}"),
					EventType::Packet(unknown) => warn!("dropping unknown packet from link: {unknown:?}"),
				}

				if bootfile_size_packet.is_some() && start_test_suite_packet.is_some() && !has_booted {
					// Send the bootfile size packet first, then the start test suite packet.
					// This is because the start test suite packet will cause the link to
					// start the test suite, which will cause it to start downloading the
					// boot file. If we send the start test suite packet first, the link
					// will start downloading the boot file before it knows how big it is,
					// which will cause it to fail.
					has_booted = true;
					trace!("disabling monitor standby");
					sender.send(Packet::SetMonitorStandby(false)).await?;
					trace!("setting scene to logo");
					sender.send(Packet::SetScene(Scene::Logo)).await?;
					trace!("informing link of bootfile sizes");
					sender.send(bootfile_size_packet.clone().take().unwrap()).await?;
					trace!("turning on the machine");
					sender.send(Packet::SetPowerState(PowerState::On)).await?;
					trace!("pressing power button");
					sender.send(Packet::PressPower).await?;
				}
			}
		}
	})()
	.await;

	warn!("link connection closed and is returning; shutting down container: {id}");

	if let Err(err) = docker.remove_container(&id, true).await {
		error!("failed to kill docker container: {:?}", err);
	}

	r
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

	let config = Arc::new(config);

	debug!("beginning server");

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
