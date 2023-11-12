#![feature(never_type)]

use async_std::{fs, io, net, prelude::*, task};
use async_tftp::packet::Packet as Tftp;
use clap::Parser;
use futures::{select, FutureExt};
use link_protocol::{channel, Packet, PowerState};
use log::{debug, error, info, trace, warn};
use rand::rngs::OsRng;
use std::{
	path::{Component, Path, PathBuf},
	time::Duration,
};

/// Serves files over TFTP to the system under test via a Link connection.
/// This is only useful for debugging and testing and isn't used by the
/// CI/CD pipeline.
#[derive(Parser)]
struct Options {
	/// Show verbose (trace) output
	#[arg(long, short = 'v', action)]
	verbose: bool,
	/// The name of the boot file (the Link requests ORO_BOOT as the entry point; this filename is
	/// rewritten to the value of this option)
	#[arg(long, short = 'b', default_value = "BOOTX64.EFI")]
	bootfile: String,
	/// The name of the pre-boot file. If specified, the first request for ORO_BOOT will start
	/// with this file, and all subsequent requests for ORO_BOOT will resolve to the normal bootfile.
	/// Useful when booting with e.g. iPXE.
	#[arg(long, short = 'p')]
	preboot: Option<String>,
	/// How long to wait before resending certain TFTP packets, in milliseconds
	/// (CAUTION: Values lower than 500ms might back up the Link board if the mobo
	/// has a slow PXE chip with high backpressure)
	#[arg(long, short = 't', default_value = "500")]
	timeout: u64,
	/// The directory to serve
	directory: String,
}

#[derive(thiserror::Error, Debug)]
enum Error {
	#[error("io error: {0}")]
	Io(#[from] io::Error),
	#[error("i/o error during protocol transcoding")]
	Proto(#[from] link_protocol::Error<io::Error>),
	#[error("i/o error during link connection negotiation")]
	RWError(
		#[from] channel::RWError<link_protocol::Error<io::Error>, link_protocol::Error<io::Error>>,
	),
	#[error("tftp parse/serialize error: {0}")]
	Tftp(#[from] async_tftp::Error),
	#[error("tftp client sent error: {0}")]
	TftpPacket(#[from] async_tftp::packet::Error),
	#[error("tftp client sent unexpected acknowledgement of block {0}")]
	UnexpectedAck(u16),
	#[error("tftp client sent unexpected data packet for block {0}")]
	UnexpectedData(u16),
	#[error("tftp client sent unexpected options ack (OACK)")]
	UnexpectedOack,
	#[error("tftp client sent unexpected write request (WRQ) (system is read-only)")]
	UnexpectedWrq,
	#[error("tftp expected an ACK packet but got unrelated packet instead")]
	ExpectedAck,
	#[error("artifact is larger than TFTP supports: {0} chunks of {0} bytes")]
	TooBig(usize, usize),
	#[error("receiver side of channel closed: {0}")]
	ChannelClosed(#[from] async_std::channel::RecvError),
}

#[async_std::main]
async fn main() -> Result<(), Error> {
	let config = Options::parse();

	if std::env::var("LEVEL").is_err() {
		std::env::set_var("LEVEL", "debug");
	}

	if config.verbose {
		std::env::set_var("LEVEL", "trace");
	}

	pretty_env_logger::try_init_timed_custom_env("LEVEL").expect("failed to initialize logger");

	trace!("creating oro link server");
	let link_server = net::TcpListener::bind(("0.0.0.0", 1337)).await?;
	let mut incoming = link_server.incoming();

	info!("listening for link connections on port 1337");

	while let Some(stream) = incoming.next().await {
		let stream = stream?;
		debug!("incoming connection");
		task::spawn({
			let root_dir = config.directory.clone();
			let bootfile = config.bootfile.clone();
			let preboot = config.preboot.clone();
			let resend_after = config.timeout;
			async move {
				if let Err(err) =
					handle_client(stream, root_dir, bootfile, preboot, resend_after).await
				{
					error!("client handler error: {err:?}");
				} else {
					debug!("client handler returned gracefully");
				}
			}
		});
	}

	warn!("oro link server shut down (no longer accepting new connections)");

	Ok(())
}

async fn handle_client(
	stream: net::TcpStream,
	root_dir: String,
	bootfile: String,
	mut preboot: Option<String>,
	resend_after: u64,
) -> Result<!, Error> {
	debug!("negotiating channel");
	let receiver = io::BufReader::new(stream.clone());
	let sender = io::BufWriter::new(stream);
	let (mut sender, mut receiver) =
		channel::negotiate(sender, receiver, &mut OsRng, channel::Side::Server).await?;
	debug!("created encrypted channel");

	let (ch_sender, ch_receiver) = async_std::channel::bounded(16);

	task::spawn(async move { while ch_sender.send(receiver.receive().await).await.is_ok() {} });

	loop {
		match ch_receiver.recv().await?? {
			Packet::Tftp(data) => {
				trace!("received tftp packet of size {}", data.len());
				let packet = async_tftp::parse::parse_packet(data.as_ref())?;
				trace!("parsed tftp packet: {packet:?}");

				match packet {
					Tftp::Ack(bid) => {
						return Err(Error::UnexpectedAck(bid));
					}
					Tftp::Data(bid, _) => {
						return Err(Error::UnexpectedData(bid));
					}
					Tftp::Error(msg) => {
						Err(msg.clone())?;
						unreachable!();
					}
					Tftp::OAck(_) => {
						return Err(Error::UnexpectedOack);
					}
					Tftp::Wrq(_) => {
						return Err(Error::UnexpectedWrq);
					}
					Tftp::Rrq(req) => {
						let artifact = if req.filename == "ORO_BOOT" {
							if let Some(preboot) = preboot.take() {
								debug!(
									"reading pre-boot entry point artifact: {} (re-written from ORO_BOOT, root: {})",
									preboot, root_dir
								);
								artifact_bytes(&root_dir, &preboot).await?
							} else {
								debug!(
									"reading entry point artifact: {} (re-written from ORO_BOOT, root: {})",
									bootfile, root_dir
								);
								artifact_bytes(&root_dir, &bootfile).await?
							}
						} else {
							debug!("reading artifact: {} (root: {})", req.filename, root_dir);
							artifact_bytes(&root_dir, &req.filename).await?
						};

						let mut opts = req.opts;
						let chunk_size = opts.block_size.unwrap_or(512) as usize;

						if opts.transfer_size.is_some() {
							opts.transfer_size = Some(artifact.len() as u64);
						}

						let opt_ack = Tftp::OAck(opts.clone());
						let buf = heapless::Vec::from_iter(opt_ack.to_bytes());
						sender.send(Packet::Tftp(buf)).await?;
						trace!("sent OACK");

						let maybe_oack_ack = ch_receiver.recv().await??;
						if let Packet::Tftp(data) = maybe_oack_ack {
							let packet = async_tftp::parse::parse_packet(data.as_ref())?;
							match packet {
								Tftp::Ack(bid) if bid == 0 => {
									debug!("got oack ack (bid=0); continuing");
								}
								Tftp::Error(err)
									if err
										== async_tftp::packet::Error::OptionsNegotiationFailed =>
								{
									debug!(
										"client rejected options; will allow client to re-negotiate"
									);
									continue;
								}
								unknown => {
									error!(
										"expected OACK acknowledgement but TFTP client sent something else: {unknown:?}"
									);
									return Err(Error::ExpectedAck);
								}
							}
						} else {
							error!(
								"expected OACK acknowledgement (ack bid=0) but got different packet: {maybe_oack_ack:?}"
							);
							return Err(Error::ExpectedAck);
						}

						let num_chunks = (artifact.len() + chunk_size - 1) / chunk_size;
						let mut offset = 0;

						if num_chunks > u16::MAX as usize {
							return Err(Error::TooBig(num_chunks, chunk_size));
						}

						for i in 1..=num_chunks {
							let new_offset = (offset + chunk_size).min(artifact.len());
							let buf = &artifact[offset..new_offset];
							let data = Tftp::Data(i as u16, buf);
							let buf = heapless::Vec::from_iter(data.to_bytes());
							offset = new_offset;

							debug!("sending block {i} of {num_chunks}");
							sender.send(Packet::Tftp(buf.clone())).await?;

							loop {
								let received_packet = select! {
									packet = ch_receiver.recv().fuse() => packet??,
									_ = task::sleep(Duration::from_millis(resend_after)).fuse() => {
										trace!("resending block {i}");
										sender.send(Packet::Tftp(buf.clone())).await?;
										continue;
									}
								};

								match received_packet {
									Packet::Tftp(ack_data) => {
										let packet = async_tftp::parse::parse_packet(&ack_data[..]);
										match packet {
											Err(err) => return Err(Error::Tftp(err)),
											Ok(Tftp::Ack(bid)) => {
												if bid == i as u16 {
													debug!("client ack'd block {i}");
													break;
												} else {
													debug!(
														"ignoring invalid ack'd block {bid} (expecting {i}); resending block {i}"
													);
													continue;
												}
											}
											Ok(Tftp::Error(err)) => {
												return Err(Error::TftpPacket(err));
											}
											Ok(unknown) => {
												warn!(
													"expected ACK during data transfer but got another TFTP packet instead: {unknown:?}"
												);
												return Err(Error::ExpectedAck);
											}
										}
									}
									unknown => {
										warn!(
											"expected ACK during data transfer but got unknown packet instead: {unknown:?}"
										);
										return Err(Error::ExpectedAck);
									}
								}
							}
						}
					}
				}
			}
			Packet::LinkOnline { uid, version } => {
				let hexid = ::hex::encode_upper(uid);
				info!("oro link came online");
				info!("    link firmware version: {}", version);
				info!("    link UID:              {}", hexid);

				debug!("retrieving bootfile size");
				let size = artifact_size(&root_dir, &bootfile).await?;
				debug!("telling link pxe service the bootfile size is {size}");
				sender.send(Packet::BootfileSize(size as u64)).await?;
				debug!("instructing the link to boot the SUT");
				sender.send(Packet::SetPowerState(PowerState::On)).await?;
				debug!("instructing the link to press the power switch");
				sender.send(Packet::PressPower).await?;
				info!("link has brought the system online; beginning tftp communication");
			}
			unknown => {
				warn!("unknown/unsupported packet sent from link: {unknown:?}");
			}
		}
	}
}

fn make_artifact_path(root: &str, filename: &str) -> PathBuf {
	Path::new(filename)
		.components()
		.filter(|c| !matches!(c, Component::RootDir))
		.fold(PathBuf::from(root), |mut r, c| {
			r.push(c);
			r
		})
}

async fn artifact_size(root: &str, filename: &str) -> Result<u64, Error> {
	let filepath = make_artifact_path(root, filename);
	let metadata = fs::metadata(filepath).await?;
	Ok(metadata.len())
}

async fn artifact_bytes(root: &str, filename: &str) -> Result<Vec<u8>, Error> {
	let filepath = make_artifact_path(root, filename);
	Ok(fs::read(filepath).await?)
}
