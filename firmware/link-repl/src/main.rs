#![feature(never_type, async_closure)]

use async_std::{io, net::TcpListener, prelude::*, sync::Mutex, task};
use envconfig::Envconfig;

use link_protocol::{
	Error as ProtoError, LogEntry, PowerState,
	channel::{PacketSender, RWError},
};
use mini_async_repl::{
	CommandStatus, Repl,
	command::{Command, CommandArgInfo, CommandArgType, ExecuteCommand},
};

use std::{str::FromStr, sync::Arc};

use async_std::{
	io::{BufReader, BufWriter},
	net::TcpStream,
};
use futures::{prelude::*, select};
use link_protocol::{Packet, Scene, channel};
use log::{error, info, warn};
use rand::rngs::OsRng;

#[derive(Envconfig, Clone)]
pub(crate) struct Config {
	#[envconfig(from = "LINK_SERVER_PORT", default = "1337")]
	pub link_server_port: u16,
	#[envconfig(from = "LINK_SERVER_BIND", default = "0.0.0.0")]
	pub link_server_bind: String,
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
async fn main() -> Result<(), Error> {
	let config = Config::init_from_env().unwrap();

	let log_level = log::LevelFilter::from_str(&config.log_level)
		.expect("failed to parse LEVEL environment variable");

	log::set_max_level(log_level);

	let mut slog = stderrlog::new();

	if config.verbose == 0 {
		slog.module(module_path!());
	}

	slog.show_module_names(true)
		.verbosity(log_level)
		.timestamp(stderrlog::Timestamp::Millisecond)
		.init()
		.expect("failed to start stderr logger");

	info!("starting oro-repl version {}", env!("CARGO_PKG_VERSION"));

	let listener =
		TcpListener::bind((config.link_server_bind.as_str(), config.link_server_port)).await?;
	let mut incoming = listener.incoming();

	info!(
		"listening for link connections on {}:{}",
		config.link_server_bind, config.link_server_port
	);

	while let Some(stream) = futures::StreamExt::next(&mut incoming).await {
		let stream = stream?;

		let (outgoing, mut incoming) = {
			// create buffered readers/writers for stream
			let sock_reader = BufReader::new(stream.clone());
			let sock_writer = BufWriter::new(stream);
			channel::negotiate(sock_writer, sock_reader, &mut OsRng, channel::Side::Server).await?
		};

		info!("established link protocol channel");

		let mut repl = make_repl(outgoing);

		let link_logger_task = task::spawn(async move {
			let packet = match incoming.receive().await {
				Ok(packet) => packet,
				Err(err) => {
					error!("failed to receive packet: {:?}", err);
					return;
				}
			};

			info!("received packet: {:?}", packet);
		});

		select! {
			_ = link_logger_task.fuse() => {
				warn!("link logger task returned");
			},
			_ = repl.run().fuse() => {
				warn!("repl exited");
				return Ok(());
			}
		}
	}

	Ok(())
}

fn make_repl(outgoing: PacketSender<BufWriter<TcpStream>>) -> Repl {
	let outgoing = Arc::new(Mutex::new(outgoing));

	Repl::builder()
		.description("Oro Link session REPL")
		.prompt("oro> ")
		.add(
			"selftest",
			Command::new(
				"performs a self-test of the REPL itself",
				vec![],
				Box::new(SelfTest()),
			),
		)
		.add(
			"scene",
			Command::new(
				"sets the scene on the link",
				vec![CommandArgInfo::new_with_name(
					CommandArgType::String,
					"scene",
				)],
				Box::new(SceneCommand(outgoing.clone())),
			),
		)
		.add(
			"monitor",
			Command::new(
				"sets the monitor standby state (on/off)",
				vec![CommandArgInfo::new_with_name(
					CommandArgType::String,
					"state",
				)],
				Box::new(MonitorCommand(outgoing.clone())),
			),
		)
		.add(
			"psu",
			Command::new(
				"sets the power state of the PSU/motherboard (on/off/standby)",
				vec![CommandArgInfo::new_with_name(
					CommandArgType::String,
					"state",
				)],
				Box::new(PowerCommand(outgoing.clone())),
			),
		)
		.add(
			"power",
			Command::new(
				"presses the power button",
				vec![],
				Box::new(PowerButtonCommand(outgoing.clone())),
			),
		)
		.add(
			"reset",
			Command::new(
				"presses the reset button",
				vec![],
				Box::new(ResetButtonCommand(outgoing.clone())),
			),
		)
		.add(
			"info",
			Command::new(
				"logs an info message",
				vec![CommandArgInfo::new_with_name(
					CommandArgType::String,
					"message",
				)],
				Box::new(InfoLogCommand(outgoing.clone())),
			),
		)
		.add(
			"warn",
			Command::new(
				"logs a warning message",
				vec![CommandArgInfo::new_with_name(
					CommandArgType::String,
					"message",
				)],
				Box::new(WarnLogCommand(outgoing.clone())),
			),
		)
		.add(
			"error",
			Command::new(
				"logs an error message",
				vec![CommandArgInfo::new_with_name(
					CommandArgType::String,
					"message",
				)],
				Box::new(ErrorLogCommand(outgoing.clone())),
			),
		)
		.add(
			"key",
			Command::new(
				"presses a key on the USB HID keyboard",
				vec![CommandArgInfo::new_with_name(
					CommandArgType::I32,
					"keycode",
				)],
				Box::new(KeyPressDebugCommand(outgoing.clone())),
			),
		)
		.add(
			"start_suite",
			Command::new(
				"starts a new test suite",
				vec![
					CommandArgInfo::new_with_name(CommandArgType::I32, "total_tests"),
					CommandArgInfo::new_with_name(CommandArgType::String, "author"),
					CommandArgInfo::new_with_name(CommandArgType::String, "title"),
					CommandArgInfo::new_with_name(CommandArgType::String, "ref_id"),
				],
				Box::new(SuiteCommand(outgoing.clone())),
			),
		)
		.add(
			"test",
			Command::new(
				"starts a new test",
				vec![CommandArgInfo::new_with_name(
					CommandArgType::String,
					"name",
				)],
				Box::new(TestCommand(outgoing)),
			),
		)
		.build()
		.unwrap()
}

struct SelfTest();

impl ExecuteCommand for SelfTest {
	fn execute(
		&mut self,
		_args: Vec<String>,
		_args_info: Vec<mini_async_repl::command::CommandArgInfo>,
	) -> std::pin::Pin<
		Box<
			dyn Future<Output = mini_async_repl::anyhow::Result<mini_async_repl::CommandStatus>>
				+ '_,
		>,
	> {
		Box::pin(async move {
			warn!("selftest command executed");
			Ok(CommandStatus::Done)
		})
	}
}

struct SceneCommand(Arc<Mutex<PacketSender<BufWriter<TcpStream>>>>);

impl ExecuteCommand for SceneCommand {
	fn execute(
		&mut self,
		args: Vec<String>,
		_args_info: Vec<mini_async_repl::command::CommandArgInfo>,
	) -> std::pin::Pin<
		Box<
			dyn Future<Output = mini_async_repl::anyhow::Result<mini_async_repl::CommandStatus>>
				+ '_,
		>,
	> {
		Box::pin(async move {
			let mut sender = self.0.lock().await;

			let scene = match args[0].as_str() {
				"test" => Scene::Test,
				"log" => Scene::Log,
				"logo" => Scene::Logo,
				unknown => {
					warn!("unknown scene: {}", unknown);
					return Ok(CommandStatus::Done);
				}
			};

			let packet = Packet::SetScene(scene);

			sender.send(packet).await?;

			Ok(CommandStatus::Done)
		})
	}
}

struct MonitorCommand(Arc<Mutex<PacketSender<BufWriter<TcpStream>>>>);

impl ExecuteCommand for MonitorCommand {
	fn execute(
		&mut self,
		args: Vec<String>,
		_args_info: Vec<mini_async_repl::command::CommandArgInfo>,
	) -> std::pin::Pin<
		Box<
			dyn Future<Output = mini_async_repl::anyhow::Result<mini_async_repl::CommandStatus>>
				+ '_,
		>,
	> {
		Box::pin(async move {
			let mut sender = self.0.lock().await;

			let standby = match args[0].as_str() {
				"on" => false,
				"off" => true,
				unknown => {
					warn!("unknown standby state: {}", unknown);
					return Ok(CommandStatus::Done);
				}
			};

			let packet = Packet::SetMonitorStandby(standby);

			sender.send(packet).await?;

			Ok(CommandStatus::Done)
		})
	}
}

struct PowerCommand(Arc<Mutex<PacketSender<BufWriter<TcpStream>>>>);

impl ExecuteCommand for PowerCommand {
	fn execute(
		&mut self,
		args: Vec<String>,
		_args_info: Vec<mini_async_repl::command::CommandArgInfo>,
	) -> std::pin::Pin<
		Box<
			dyn Future<Output = mini_async_repl::anyhow::Result<mini_async_repl::CommandStatus>>
				+ '_,
		>,
	> {
		Box::pin(async move {
			let mut sender = self.0.lock().await;

			let state = match args[0].as_str() {
				"on" => PowerState::On,
				"off" => PowerState::Off,
				"standby" => PowerState::Standby,
				unknown => {
					warn!("unknown power state: {}", unknown);
					return Ok(CommandStatus::Done);
				}
			};

			let packet = Packet::SetPowerState(state);

			sender.send(packet).await?;

			Ok(CommandStatus::Done)
		})
	}
}

struct PowerButtonCommand(Arc<Mutex<PacketSender<BufWriter<TcpStream>>>>);

impl ExecuteCommand for PowerButtonCommand {
	fn execute(
		&mut self,
		_args: Vec<String>,
		_args_info: Vec<mini_async_repl::command::CommandArgInfo>,
	) -> std::pin::Pin<
		Box<
			dyn Future<Output = mini_async_repl::anyhow::Result<mini_async_repl::CommandStatus>>
				+ '_,
		>,
	> {
		Box::pin(async move {
			let mut sender = self.0.lock().await;

			let packet = Packet::PressPower;

			sender.send(packet).await?;

			Ok(CommandStatus::Done)
		})
	}
}

struct ResetButtonCommand(Arc<Mutex<PacketSender<BufWriter<TcpStream>>>>);

impl ExecuteCommand for ResetButtonCommand {
	fn execute(
		&mut self,
		_args: Vec<String>,
		_args_info: Vec<mini_async_repl::command::CommandArgInfo>,
	) -> std::pin::Pin<
		Box<
			dyn Future<Output = mini_async_repl::anyhow::Result<mini_async_repl::CommandStatus>>
				+ '_,
		>,
	> {
		Box::pin(async move {
			let mut sender = self.0.lock().await;

			let packet = Packet::PressReset;

			sender.send(packet).await?;

			Ok(CommandStatus::Done)
		})
	}
}

struct InfoLogCommand(Arc<Mutex<PacketSender<BufWriter<TcpStream>>>>);

impl ExecuteCommand for InfoLogCommand {
	fn execute(
		&mut self,
		args: Vec<String>,
		_args_info: Vec<mini_async_repl::command::CommandArgInfo>,
	) -> std::pin::Pin<
		Box<
			dyn Future<Output = mini_async_repl::anyhow::Result<mini_async_repl::CommandStatus>>
				+ '_,
		>,
	> {
		Box::pin(async move {
			let mut sender = self.0.lock().await;
			let packet = Packet::Log(LogEntry::Info(heapless::String::<255>::from_iter(
				args.join(" ").chars(),
			)));
			sender.send(packet).await?;
			Ok(CommandStatus::Done)
		})
	}
}

struct WarnLogCommand(Arc<Mutex<PacketSender<BufWriter<TcpStream>>>>);

impl ExecuteCommand for WarnLogCommand {
	fn execute(
		&mut self,
		args: Vec<String>,
		_args_info: Vec<mini_async_repl::command::CommandArgInfo>,
	) -> std::pin::Pin<
		Box<
			dyn Future<Output = mini_async_repl::anyhow::Result<mini_async_repl::CommandStatus>>
				+ '_,
		>,
	> {
		Box::pin(async move {
			let mut sender = self.0.lock().await;
			let packet = Packet::Log(LogEntry::Warn(heapless::String::<255>::from_iter(
				args.join(" ").chars(),
			)));
			sender.send(packet).await?;
			Ok(CommandStatus::Done)
		})
	}
}

struct ErrorLogCommand(Arc<Mutex<PacketSender<BufWriter<TcpStream>>>>);

impl ExecuteCommand for ErrorLogCommand {
	fn execute(
		&mut self,
		args: Vec<String>,
		_args_info: Vec<mini_async_repl::command::CommandArgInfo>,
	) -> std::pin::Pin<
		Box<
			dyn Future<Output = mini_async_repl::anyhow::Result<mini_async_repl::CommandStatus>>
				+ '_,
		>,
	> {
		Box::pin(async move {
			let mut sender = self.0.lock().await;
			let packet = Packet::Log(LogEntry::Error(heapless::String::<255>::from_iter(
				args.join(" ").chars(),
			)));
			sender.send(packet).await?;
			Ok(CommandStatus::Done)
		})
	}
}

struct KeyPressDebugCommand(Arc<Mutex<PacketSender<BufWriter<TcpStream>>>>);

impl ExecuteCommand for KeyPressDebugCommand {
	fn execute(
		&mut self,
		args: Vec<String>,
		_args_info: Vec<mini_async_repl::command::CommandArgInfo>,
	) -> std::pin::Pin<
		Box<
			dyn Future<Output = mini_async_repl::anyhow::Result<mini_async_repl::CommandStatus>>
				+ '_,
		>,
	> {
		Box::pin(async move {
			let mut sender = self.0.lock().await;
			let packet = Packet::DebugUsbKey(
				args[0]
					.parse()
					.map_err(|_| mini_async_repl::anyhow::anyhow!("invalid keycode"))?,
			);
			sender.send(packet).await?;
			Ok(CommandStatus::Done)
		})
	}
}

struct SuiteCommand(Arc<Mutex<PacketSender<BufWriter<TcpStream>>>>);

impl ExecuteCommand for SuiteCommand {
	fn execute(
		&mut self,
		args: Vec<String>,
		_args_info: Vec<mini_async_repl::command::CommandArgInfo>,
	) -> std::pin::Pin<
		Box<
			dyn Future<Output = mini_async_repl::anyhow::Result<mini_async_repl::CommandStatus>>
				+ '_,
		>,
	> {
		Box::pin(async move {
			let mut sender = self.0.lock().await;
			let packet = Packet::StartTestSession {
				total_tests: args[0]
					.parse()
					.map_err(|_| mini_async_repl::anyhow::anyhow!("invalid total_tests"))?,
				author: args[1]
					.as_str()
					.try_into()
					.map_err(|_| mini_async_repl::anyhow::anyhow!("author too long"))?,
				title: args[2]
					.as_str()
					.try_into()
					.map_err(|_| mini_async_repl::anyhow::anyhow!("title too long"))?,
				ref_id: args[3]
					.as_str()
					.try_into()
					.map_err(|_| mini_async_repl::anyhow::anyhow!("ref_id too long"))?,
			};
			sender.send(packet).await?;
			Ok(CommandStatus::Done)
		})
	}
}

struct TestCommand(Arc<Mutex<PacketSender<BufWriter<TcpStream>>>>);

impl ExecuteCommand for TestCommand {
	fn execute(
		&mut self,
		args: Vec<String>,
		_args_info: Vec<mini_async_repl::command::CommandArgInfo>,
	) -> std::pin::Pin<
		Box<
			dyn Future<Output = mini_async_repl::anyhow::Result<mini_async_repl::CommandStatus>>
				+ '_,
		>,
	> {
		Box::pin(async move {
			let mut sender = self.0.lock().await;
			let packet = Packet::StartTest {
				name: args[0]
					.as_str()
					.try_into()
					.map_err(|_| mini_async_repl::anyhow::anyhow!("name too long"))?,
			};
			sender.send(packet).await?;
			Ok(CommandStatus::Done)
		})
	}
}
