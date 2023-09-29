//! # Security
//!
//! This is a wildly insecure, leaky, messy bit of code.
//! It's not meant to be production ready. It ABSOLUTELY
//! IS vulnerable to an untrusted client allocating gigabytes
//! of memory remotely, without authentication in place whatsoever.
//!
//! **DO NOT - I REPEAT - DO _NOT_ RUN THIS IN AN UNTRUSTED
//! NETWORK OR ENVIRONMENT.**
#![feature(async_fn_in_trait)]

mod rpcap;

use async_broadcast::{broadcast as make_broadcast, InactiveReceiver as InactiveBroadcastReceiver};
use async_std::{
	channel::{self, Sender},
	io::{self, ReadExt},
	net::{TcpListener, TcpStream},
	prelude::*,
	task,
};
use clap::Parser;
use futures_lite::future::FutureExt;
use std::time::SystemTime;

#[macro_use]
extern crate log;

/// Allows capturing packets from an Oro Link board
/// over the auxiliary serial port to be sent over
/// and RPCAP TCP connection to e.g. WireShark.
///
/// Pipe serial port output into stdin of this utility.
#[derive(clap::Parser, Debug)]
struct Options {
	/// Turns on verbose logging (can also be set with
	/// `LEVEL=trace`, etc). This flag overrides `LEVEL`.
	#[arg(short, long)]
	verbose: bool,

	/// The address on which to bind the RPCAP server
	#[arg(short = 'H', long = "host", default_value_t = {"0.0.0.0".to_string()})]
	bind_host: String,

	/// The port on which to bind the RPCAP server
	#[arg(short = 'p', long = "port", default_value_t = 2002)]
	bind_port: u16,
}

#[derive(Clone, Debug)]
enum Event {
	Frame {
		data: Vec<u8>,
		arrival_time: SystemTime,
		number: usize,
	},
	Control(rpcap::RPCAPMessage),
}

async fn stdin_task(sender: Sender<Event>) {
	let mut stdin = io::stdin();
	debug!("beginning stdin reader loop");

	let mut count = 0;

	loop {
		// read two bytes
		let mut len_buf = [0u8; 2];
		stdin.read_exact(&mut len_buf[..]).await.unwrap();
		let len = u16::from_be_bytes(len_buf) as usize;
		debug!("received byte length prefix: {len}");
		let mut frame = vec![0u8; len];
		// read rest of frame
		stdin.read_exact(&mut frame[..]).await.unwrap();
		debug!("received frame");
		sender
			.send(Event::Frame {
				data: frame,
				arrival_time: SystemTime::now(),
				number: count,
			})
			.await
			.unwrap();
		count += 1;
		debug!("forwarded frame to broker");
	}
}

async fn server_task(
	bind_host: String,
	bind_port: u16,
	receiver: InactiveBroadcastReceiver<Event>,
) {
	let listener = TcpListener::bind((bind_host.as_str(), bind_port))
		.await
		.unwrap();
	let mut incoming = listener.incoming();
	info!("listening on {}:{}", bind_host, bind_port);

	while let Some(stream) = incoming.next().await {
		let stream = stream.unwrap();

		match stream.peer_addr() {
			Ok(addr) => info!("incoming connection from {:?}", addr),
			Err(_) => info!("incoming connection from unknown origin"),
		}

		task::spawn({
			let receiver = receiver.clone();
			let bind_host = bind_host.clone();
			async move {
				if let Err(err) = peer_task(stream, bind_host, receiver).await {
					error!("client connection errored: {err:?}");
				}
			}
		});
	}
}

async fn peer_task(
	mut stream: TcpStream,
	bind_host: String,
	receiver: InactiveBroadcastReceiver<Event>,
) -> Result<(), io::Error> {
	let s = &mut stream;

	loop {
		match rpcap::RPCAPMessage::parse(s).await? {
			rpcap::RPCAPMessage::AuthRequest { auth_type } => {
				debug!("client attempted to authenticate with {auth_type:?} auth mechanism");
				match auth_type {
					rpcap::AuthType::Null => {
						debug!("client authenticated with null authentication (good)");
						rpcap::RPCAPMessage::AuthResponse.encode(s).await?;
					}
					_ => {
						warn!(
							"this daemon doesn't support this auth type; only Null auth is supported!"
						);
						rpcap::RPCAPMessage::AuthNotSupError(
							"only Null authentication is supported by Oro Link".to_string(),
						)
						.encode(s)
						.await?;
					}
				}
			}

			rpcap::RPCAPMessage::OpenDeviceRequest { device_name } => {
				debug!("client requested open device: {device_name}");
				if device_name == "oro" {
					rpcap::RPCAPMessage::OpenDeviceResponse.encode(s).await?;
				} else {
					warn!("only the device 'oro' is supported");
					rpcap::RPCAPMessage::OpenError(
						"only the device 'oro' is supported".to_string(),
					)
					.encode(s)
					.await?;
				}
			}

			rpcap::RPCAPMessage::FindAllDevsRequest => {
				debug!("client requested list of all devices");
				rpcap::RPCAPMessage::FindAllDevsResponse(vec![rpcap::Interface {
					name: "oro".to_string(),
					description: "Oro Link to SUT ethernet interface".to_string(),
				}])
				.encode(s)
				.await?;
			}

			rpcap::RPCAPMessage::StartCapRequest => {
				debug!("client requested a start capture");

				let client_listener = TcpListener::bind((bind_host.as_str(), 0)).await.unwrap();
				let local_port = client_listener.local_addr()?.port();

				task::spawn(capture_server_task(client_listener, receiver.clone()));

				rpcap::RPCAPMessage::StartCapResponse {
					server_port: local_port,
				}
				.encode(s)
				.await?;
			}

			rpcap::RPCAPMessage::UpdateFilterRequest => {
				debug!("client requested a BPF filter update (will ignore it)");
				rpcap::RPCAPMessage::UpdateFilterResponse.encode(s).await?;
			}

			msg => {
				warn!(
					"supported message came but we weren't expecting it; killing client: {msg:?}"
				);
				rpcap::RPCAPMessage::WrongMessageError("unknown request from client".to_string())
					.encode(s)
					.await?;
				break;
			}
		}
	}

	info!("client is closing");
	Ok(())
}

trait RPCAPEventEmitter {
	async fn next_event(&mut self) -> Result<Event, io::Error>;
}

impl<T> RPCAPEventEmitter for T
where
	T: ReadExt + Unpin + Sized,
{
	async fn next_event(&mut self) -> Result<Event, io::Error> {
		Ok(Event::Control(rpcap::RPCAPMessage::parse(self).await?))
	}
}

async fn capture_server_task(listener: TcpListener, receiver: InactiveBroadcastReceiver<Event>) {
	let mut incoming = listener.incoming();
	let stream = match incoming.next().await {
		None => {
			error!("tcp listener failed to listen for clients for some reason...");
			return;
		}
		Some(stream) => stream.unwrap(),
	};
	drop(incoming);
	drop(listener);

	match stream.peer_addr() {
		Ok(addr) => info!("incoming packet capture connection from {:?}", addr),
		Err(_) => info!("incoming packet capture connection from unknown origin"),
	}

	let mut receiver = receiver.activate();
	let (reader, writer) = &mut (&stream, &stream);

	loop {
		let event = (async { Ok(receiver.recv().await.unwrap()) })
			.race(reader.next_event())
			.await;
		let event = match event {
			Ok(ev) => ev,
			Err(err) => {
				warn!("capture client errored; terminating: {err:?}");
				return;
			}
		};

		match event {
			Event::Frame {
				data,
				arrival_time,
				number,
			} => {
				if let Err(err) = (rpcap::RPCAPMessage::Packet {
					data,
					arrival_time,
					number,
				})
				.encode(writer)
				.await
				{
					error!("failed to send captured packet to capture socket: {err:?}");
					return;
				}
			}
			ev => {
				warn!("got unexpected event from capture client; ignoring: {ev:?}");
			}
		}
	}
}

async fn pmain() -> Result<(), io::Error> {
	eprintln!("**SECURITY WARNING**: Do NOT use this utility in any configuration or network");
	eprintln!(
		"**SECURITY WARNING**: where untrusted connections may be received. YOU HAVE BEEN WARNED."
	);

	let config = Options::parse();

	if std::env::var("LEVEL").is_err() {
		std::env::set_var("LEVEL", "info");
	}

	if config.verbose {
		std::env::set_var("LEVEL", "trace");
	}

	pretty_env_logger::try_init_timed_custom_env("LEVEL").expect("failed to initialize logger");

	let (sender, receiver) = channel::unbounded();
	let (broadcast_sender, broadcast_receiver) = make_broadcast(256);

	task::spawn(stdin_task(sender.clone()));
	task::spawn(server_task(
		config.bind_host,
		config.bind_port,
		broadcast_receiver.deactivate(),
	));

	loop {
		broadcast_sender
			.broadcast(receiver.recv().await.unwrap())
			.await
			.unwrap();
	}
}

#[async_std::main]
async fn main() {
	if let Err(err) = pmain().await {
		error!("fatal error: {}", err);
	}
}
