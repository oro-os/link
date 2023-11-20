use crate::{docker::Docker, Config, Error};
use async_std::{
	channel::{bounded as make_bounded_channel, Receiver, Sender},
	fs,
	io::{BufReader, BufWriter, ErrorKind},
	net::TcpStream,
	os::unix::net::UnixListener,
	task::{self},
};
use futures::{prelude::*, select};
use link_protocol::{channel, Packet, PowerState, Scene};
use log::{debug, error, info, trace, warn};
use rand::rngs::OsRng;
use std::os::unix::fs::PermissionsExt;

macro_rules! race_all_or_cancel {
	($f1:expr) => {
		$f1.await
	};
	($($f:expr),*) => {{
		let (r, _, rest) = ::futures::future::select_all([
			$($f,)*
		])
		.await;

		for f in rest {
			let _ = f.cancel().await;
		}

		r
	}};
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum ControlMessage {
	EstablishedLink { id: String },
	EstablishedServer { path: String },
	Packet(Packet),
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum BrokerMessage {
	Link(ControlMessage),
	Client(ControlMessage),
}

pub(crate) async fn run_session(config: Config, link_stream: TcpStream) -> Result<(), Error> {
	let (broker_sender, broker_receiver) = make_bounded_channel(32);
	let (link_sender, link_receiver) = make_bounded_channel(32);
	let (client_sender, client_receiver) = make_bounded_channel(32);

	let link_handle = task::spawn(handle_link(
		link_stream,
		broker_sender.clone(),
		link_receiver,
	));

	// wait for the link to indicate it's established a connection
	let link_id = match broker_receiver.recv().await? {
		BrokerMessage::Link(ControlMessage::EstablishedLink { id }) => id,
		_ => return Err(Error::NoHelloPacket),
	};

	// start the UDS server for the github actions runner
	let client_handle = task::spawn(handle_client(
		link_id.clone(),
		broker_sender.clone(),
		client_receiver,
	));

	// wait for the client to indicate it's established a connection
	let client_path = match broker_receiver.recv().await? {
		BrokerMessage::Client(ControlMessage::EstablishedServer { path }) => path,
		_ => panic!("unexpected message from client"),
	};

	// start the docker container
	let docker_handle = task::spawn(handle_docker(config, link_id.clone(), client_path));

	// start the broker
	let broker_handle = task::spawn(handle_broker(broker_receiver, link_sender, client_sender));

	race_all_or_cancel!(link_handle, client_handle, docker_handle, broker_handle)
}

async fn handle_broker(
	broker: Receiver<BrokerMessage>,
	link: Sender<ControlMessage>,
	client: Sender<ControlMessage>,
) -> Result<(), Error> {
	debug!("starting broker");

	let mut has_sent_bootfile_size = false;
	let mut has_sent_test_session = false;
	let mut has_started_test_session = false;
	let mut has_started_first_test = false;

	loop {
		match broker.recv().await? {
			BrokerMessage::Link(ControlMessage::Packet(Packet::Serial(data))) => {
				client
					.send(ControlMessage::Packet(Packet::Serial(data)))
					.await?;
			}
			BrokerMessage::Client(ControlMessage::Packet(Packet::Serial(data))) => {
				link.send(ControlMessage::Packet(Packet::Serial(data)))
					.await?;
			}
			BrokerMessage::Client(ControlMessage::Packet(Packet::BootfileSize { uefi, bios })) => {
				link.send(ControlMessage::Packet(Packet::BootfileSize { uefi, bios }))
					.await?;
				has_sent_bootfile_size = true;
			}
			BrokerMessage::Client(ControlMessage::Packet(Packet::PressPower)) => {
				link.send(ControlMessage::Packet(Packet::PressPower))
					.await?;
			}
			BrokerMessage::Client(ControlMessage::Packet(Packet::PressReset)) => {
				link.send(ControlMessage::Packet(Packet::PressReset))
					.await?;
			}
			BrokerMessage::Link(ControlMessage::Packet(Packet::Tftp(data))) => {
				client
					.send(ControlMessage::Packet(Packet::Tftp(data)))
					.await?;
			}
			BrokerMessage::Client(ControlMessage::Packet(Packet::Tftp(data))) => {
				link.send(ControlMessage::Packet(Packet::Tftp(data)))
					.await?;
			}
			BrokerMessage::Client(ControlMessage::Packet(Packet::StartTest { name })) => {
				if !has_started_first_test {
					has_started_first_test = true;

					// Switch to testing scene
					link.send(ControlMessage::Packet(Packet::SetScene(Scene::Test)))
						.await?;
				}

				link.send(ControlMessage::Packet(Packet::StartTest { name }))
					.await?;
			}
			BrokerMessage::Client(ControlMessage::Packet(Packet::StartTestSession {
				total_tests,
				author,
				title,
				ref_id,
			})) => {
				link.send(ControlMessage::Packet(Packet::StartTestSession {
					total_tests,
					author,
					title,
					ref_id,
				}))
				.await?;
				has_sent_test_session = true;
			}
			unknown => {
				error!("unexpected message sent to broker: {unknown:?}");
				return Err(Error::UnexpectedPacket);
			}
		}

		if has_sent_bootfile_size && has_sent_test_session && !has_started_test_session {
			has_started_test_session = true;

			// Turn on the monitor
			link.send(ControlMessage::Packet(Packet::SetMonitorStandby(false)))
				.await?;
			// Then set the scene to the logo
			link.send(ControlMessage::Packet(Packet::SetScene(Scene::Logo)))
				.await?;
			// Turn on the machine
			link.send(ControlMessage::Packet(Packet::SetPowerState(
				PowerState::On,
			)))
			.await?;
			// Press the power button
			link.send(ControlMessage::Packet(Packet::PressPower))
				.await?;
		}
	}
}

async fn handle_link(
	stream: TcpStream,
	broker: Sender<BrokerMessage>,
	receiver: Receiver<ControlMessage>,
) -> Result<(), Error> {
	info!("starting link connection");

	let (mut outgoing, mut incoming) = {
		// create buffered readers/writers for stream
		let sock_reader = BufReader::new(stream.clone());
		let sock_writer = BufWriter::new(stream);
		channel::negotiate(sock_writer, sock_reader, &mut OsRng, channel::Side::Server).await?
	};

	info!("established link protocol channel");

	// wait for first packet - the hello packet - from the link
	let hello = incoming.receive().await?;
	if let Packet::LinkOnline { uid, version } = hello {
		let id = hex::encode_upper(&uid[..]);
		info!("link online: {id} (firmware version {version})");
		broker
			.send(BrokerMessage::Link(ControlMessage::EstablishedLink { id }))
			.await?;
	} else {
		error!("unexpected packet from link: {hello:?}");
		return Err(Error::NoHelloPacket);
	}

	debug!("link connection negotiated; waiting for packets");

	loop {
		select! {
			packet = incoming.receive().fuse() => {
				trace!("link -> broker: {packet:?}");
				broker.send(BrokerMessage::Link(ControlMessage::Packet(packet?))).await?;
			},
			packet = receiver.recv().fuse() => match packet? {
				ControlMessage::Packet(packet) => {
					trace!("broker -> link: {packet:?}");
					outgoing.send(packet).await?;
				},
				unknown => panic!("unexpected message from broker: {unknown:?}")
			}
		}
	}
}

async fn handle_client(
	link_id: String,
	broker: Sender<BrokerMessage>,
	receiver: Receiver<ControlMessage>,
) -> Result<(), Error> {
	info!("starting github actions runner server");

	let socket_path = format!("/tmp/link-{link_id}.sock");

	match fs::remove_file(&socket_path).await {
		Ok(()) => {
			debug!("removed existing socket file: {socket_path}");
		}
		Err(e) if e.kind() == ErrorKind::NotFound => {
			debug!("no existing socket file to clean: {socket_path}");
		}
		Err(e) => {
			warn!("failed to remove existing socket file: {e}");
			Err(e)?;
			unreachable!();
		}
	}

	let server = UnixListener::bind(&socket_path).await?;
	debug!("setting permissions for socket: {socket_path}");
	fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o777)).await?;
	info!("listening on {socket_path}");

	broker
		.send(BrokerMessage::Client(ControlMessage::EstablishedServer {
			path: socket_path,
		}))
		.await?;

	let (stream, _) = server.accept().await?;
	drop(server);

	info!("accepted connection from github actions runner");

	let (mut outgoing, mut incoming) = {
		let (sock_reader, sock_writer) = stream.split();
		// create buffered readers/writers for stream
		let sock_reader = BufReader::new(sock_reader);
		let sock_writer = BufWriter::new(sock_writer);
		channel::negotiate(sock_writer, sock_reader, &mut OsRng, channel::Side::Server).await?
	};

	loop {
		select! {
			packet = incoming.receive().fuse() => {
				trace!("client -> broker: {packet:?}");
				broker.send(BrokerMessage::Client(ControlMessage::Packet(packet?))).await?;
			},
			packet = receiver.recv().fuse() => match packet? {
				ControlMessage::Packet(packet) => {
					trace!("broker -> client: {packet:?}");
					outgoing.send(packet).await?;
				},
				unknown => panic!("unexpected message from broker: {unknown:?}")
			}
		}
	}
}

async fn handle_docker(config: Config, link_id: String, socket_path: String) -> Result<(), Error> {
	let docker = Docker::new(&config.docker_host)?;

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
		.create_container(&crate::docker::CreateContainer {
			image: config.docker_ref.clone(),
			labels: Some(
				crate::docker::Map::new()
					.add("sh.oro".into(), "link".into())
					.add("sh.oro.link".into(), link_id.clone()),
			),
			env: Some(
				crate::docker::Args::new()
					.add("ACCESS_TOKEN".into(), config.gh_access_token.clone())
					.add("ORGANIZATION".into(), config.gh_organization.clone())
					.add("LABELS".into(), "self-hosted,oro,oro-link,x86_64".into()) // TODO(qix-): use self-report functionality of link
					.add("NAME".into(), link_id.clone()),
			),
			host_config: Some(crate::docker::HostConfig {
				binds: Some(crate::docker::Binds(vec![(
					socket_path,
					"/oro-link.sock".into(),
					Some("rw".into()),
				)])),
			}),
			..Default::default()
		})
		.await?;

	let mut container_guard = ContainerGuard {
		docker: &docker,
		id: Some(id.clone()),
	};

	debug!("created actions runner container; starting the container: {id}");
	docker.start_container(&id).await?;

	debug!("container started; waiting for exit: {id}");
	docker.wait_for_container(&id).await?;
	warn!("actions runner container exited");

	debug!("removing container: {id}");
	docker.remove_container(&id, true).await?;
	debug!("removed container: {id}");

	// disarm the container guard
	container_guard.id = None;

	Ok(())
}

struct ContainerGuard<'a> {
	docker: &'a Docker,
	id: Option<String>,
}

impl<'a> Drop for ContainerGuard<'a> {
	fn drop(&mut self) {
		if let Some(id) = self.id.clone() {
			let docker = self.docker.clone();
			debug!("dropping container guard; killing container: {id}");
			task::spawn(async move {
				if let Err(err) = docker.remove_container(&id, true).await {
					error!("failed to kill docker container: {:?}", err);
				}
			});
		}
	}
}
