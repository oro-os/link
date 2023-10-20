#![feature(never_type, async_fn_in_trait)]

use async_std::{
	io,
	net::{TcpListener, TcpStream},
	prelude::*,
	task,
};
use curve25519::curve25519_pk;
use envconfig::Envconfig;
use heapless::Vec;
use log::{debug, error, info, warn};
use rand::{rngs::OsRng, RngCore};

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
	use aes::cipher::KeyInit;
	debug!("incoming oro link connection");

	// Begin encryption negotiation
	let mut sk = [0u8; 32];
	OsRng.fill_bytes(&mut sk);
	let sk = curve25519::curve25519_sk(sk);
	let pk = curve25519_pk(sk);
	let mut their_pk = [0u8; 32];
	stream.read_exact(&mut their_pk[..]).await?;
	stream.write_all(&pk[..]).await?;
	let key = curve25519::curve25519(sk, their_pk);
	let enc = aes::Aes256Enc::new_from_slice(&key[..]).unwrap();
	let dec = aes::Aes256Dec::new_from_slice(&key[..]).unwrap();

	debug!("oro link peer encryption session negotiated");

	let mut block = [0u8; 16];
	stream.read_exact(&mut block[..]).await?;
	debug!("got bytes: {:?}", block);
	use aes::cipher::BlockDecrypt;
	dec.decrypt_block((&mut block).into());
	debug!(
		"got message: {}",
		core::str::from_utf8(&block[0..8]).unwrap_or("<bad string>")
	);

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

	// XXX DEBUG
	{
		let mut buffer: Vec<u8, 2048> = Vec::new();
		use ::link_protocol::{Deserialize, Serialize};
		let src = ::link_protocol::LinkPacket::LinkOnline {
			uid: [42; 32],
			version: env!("CARGO_PKG_VERSION").into(),
		};
		info!("SRC = {:#?}", src);
		let r = {
			let mut writer = VecWriter::new(&mut buffer);
			src.serialize(&mut writer).await
		};
		match r {
			Ok(()) => {
				info!("OK, serialized source: {:?}", &buffer[..]);
				let mut reader = VecReader::new(&buffer);
				match ::link_protocol::LinkPacket::deserialize(&mut reader).await {
					Ok(dst) => {
						info!("OK, deserialized = {:#?}", dst);
					}
					Err(err) => {
						error!("failed to deserialize dst: {:?}", err);
					}
				}
			}
			Err(err) => error!("failed to serialize src: {:?}", err),
		}
	}

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

/* XXX just some test code */
pub struct VecWriter<'a, const SZ: usize> {
	buffer: &'a mut Vec<u8, SZ>,
}

impl<'a, const SZ: usize> VecWriter<'a, SZ> {
	pub fn new(buffer: &'a mut Vec<u8, SZ>) -> Self {
		VecWriter { buffer }
	}
}

impl<'a, const SZ: usize> link_protocol::Write for VecWriter<'a, SZ> {
	async fn write(&mut self, buf: &[u8]) -> Result<(), link_protocol::Error> {
		if self.buffer.len() + buf.len() <= SZ {
			self.buffer
				.extend_from_slice(buf)
				.map_err(|_| link_protocol::Error::Eof)?;
			Ok(())
		} else {
			Err(link_protocol::Error::Eof)
		}
	}
}

pub struct VecReader<'a, const SZ: usize> {
	buffer: &'a Vec<u8, SZ>,
	pos: usize,
}

impl<'a, const SZ: usize> VecReader<'a, SZ> {
	pub fn new(buffer: &'a Vec<u8, SZ>) -> Self {
		VecReader { buffer, pos: 0 }
	}
}

impl<'a, const SZ: usize> link_protocol::Read for VecReader<'a, SZ> {
	async fn read(&mut self, buf: &mut [u8]) -> Result<(), link_protocol::Error> {
		if self.pos < self.buffer.len() {
			let available = self.buffer.len() - self.pos;
			let to_copy = core::cmp::min(available, buf.len());
			let src_slice = &self.buffer[self.pos..self.pos + to_copy];
			buf[0..to_copy].copy_from_slice(src_slice);
			self.pos += to_copy;
			Ok(())
		} else {
			Err(link_protocol::Error::Eof)
		}
	}
}
