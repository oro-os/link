#![feature(never_type, async_fn_in_trait)]

use async_std::{
	io,
	net::{TcpListener, TcpStream},
	prelude::*,
	task,
};
use envconfig::Envconfig;
use heapless::Vec;
use log::{debug, error, info, warn};
use rand::rngs::OsRng;

#[derive(Envconfig)]
struct Config {
	#[envconfig(from = "LINK_SERVER_PORT", default = "1337")]
	pub link_server_port: u16,
	#[envconfig(from = "LINK_SERVER_BIND", default = "0.0.0.0")]
	pub link_server_bind: String,
	#[cfg(target_os = "linux")]
	#[envconfig(from = "USE_JOURNALD", default = "0")]
	pub use_journald: u8,
}

async fn task_process_oro_link(mut stream: TcpStream) -> Result<(), io::Error> {
	debug!("incoming oro link connection");
	todo!("use link-protocol channels");
	Ok(())
}

async fn task_accept_oro_link_tcp(bind_host: String, port: u16) -> Result<(), io::Error> {
	let listener = TcpListener::bind((bind_host.as_str(), port)).await?;
	let mut incoming = listener.incoming();

	info!("listening for link connections on {}:{}", bind_host, port);

	while let Some(stream) = incoming.next().await {
		let stream = stream?;
		task::spawn(async move {
			if let Err(err) = task_process_oro_link(stream).await {
				error!("oro link peer connection encountered error: {:?}", err);
			}
		});
	}

	Ok(())
}

#[async_std::main]
async fn main() -> Result<!, io::Error> {
	let config = Config::init_from_env().unwrap();

	log::set_max_level(log::LevelFilter::Trace);

	let should_fallback = true;

	#[cfg(target_os = "linux")]
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

	if should_fallback {
		stderrlog::new()
			.module(module_path!())
			.verbosity(log::max_level())
			.timestamp(stderrlog::Timestamp::Millisecond)
			.init()
			.expect("failed to start stderr logger");
	}

	info!("starting oro-linkd version {}", env!("CARGO_PKG_VERSION"));

	task::spawn(async move {
		if let Err(err) =
			task_accept_oro_link_tcp(config.link_server_bind, config.link_server_port).await
		{
			error!("oro link tcp server error: {:?}", err);
		}
		warn!("oro link tcp server has shut down; terminating...");
		std::process::exit(2);
	});

	async_io::Timer::never().await;
	unreachable!();
}
