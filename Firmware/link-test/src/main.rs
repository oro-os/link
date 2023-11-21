#![feature(never_type)]

use async_std::{fs, io, os::unix::net::UnixStream, prelude::*, process, task};
use async_tftp::packet::Packet as Tftp;
use clap::Parser;
use futures::{select, FutureExt};
use link_protocol::{
	channel::{self, RWError, Side as ChannelSide},
	Error as PacketError, Packet,
};
use log::{debug, error, info, trace, warn};
use std::{
	path::{Component, Path, PathBuf},
	time::Duration,
};

/// Runs a test session on an Oro Link.
///
/// A command is executed and a test session is initiated on
/// the link. The `--pxe` directory is read for PXE booting,
/// and a series of pipes are set up to enable the child process
/// to control and communicate with the link and the running system.
///
/// FD0 (stdin) is the serial output from the machine. FD2 is the serial
/// input to the machine. FD1 is the control socket for the link, through
/// which you can issue commands to control the operation of the link itself.
///
/// UTF-8 is used for all communication.
///
/// FD1 commands are NL-delimited strings. The following commands are supported:
///
/// power
///    Presses the power button of the machine.
/// reset
///    Presses the reset button of the machine.
/// echo <string>
///    Echoes the string to stderr. This is purely for display purposes.
/// test <test-name>
///    Tells the link a new test is being started. This is purely for display purposes.
///    The test name is a UTF-8 string of up to 255 bytes. It is automatically output
///    to stderr.
/// pass
///    Tells the link the current test has passed. This is purely for display purposes.
/// fail
///    Tells the link the current test has failed. This is purely for display purposes.
/// skip
///    Tells the link the current test is being skipped. This is purely for display purposes.
#[derive(Parser)]
struct Config {
	/// The absolute path to the Oro Link socket file
	#[arg(long = "socket", default_value = "/oro-link.sock")]
	socket_path: String,
	/// The directory from which PXE files will be served
	#[arg(long = "pxe", default_value = "/oro")]
	pxe_dir: String,
	/// The entry point for the PXE boot when the machine is booted in UEFI mode
	#[arg(long = "pxe-uefi", default_value = "BOOTX64.EFI")]
	pxe_entry_uefi: String,
	/// The entry point for the PXE boot when the machine is booted in BIOS mode
	#[arg(long = "pxe-bios", default_value = "bios.bin")]
	pxe_entry_bios: String,
	/// The name of the test session (max 255 UTF-8 bytes)
	#[arg(long = "name")]
	session_name: String,
	/// The name of the test session's author (max 255 UTF-8 bytes)
	#[arg(long = "author")]
	session_author: String,
	/// The reference ID of the test session (branch name, commit SHA, etc.) (max 255 UTF-8 bytes)
	#[arg(long = "ref")]
	session_ref: String,
	/// The total number of tests that will be run
	#[arg(long = "num-tests")]
	session_num_tests: u32,
	/// Show verbose output
	#[arg(long = "verbose", short = 'v', action)]
	verbose: bool,
	/// The command to run
	#[arg(last = true)]
	cmd: Vec<String>,
	/// TFTP data block timeout in milliseconds
	#[arg(long = "timeout", default_value = "500")]
	timeout: u64,
	/// Whether to output Github Actions control messages along with certain output
	#[arg(long = "github-actions", short = 'g', action)]
	github_actions: bool,
}

#[derive(Debug, thiserror::Error)]
enum Error {
	#[error("IO error")]
	Io(#[from] io::Error),
	#[error("link packet error")]
	Packet(#[from] PacketError<io::Error>),
	#[error("IO error during link connection negotiation")]
	RWError(#[from] RWError<PacketError<io::Error>, PacketError<io::Error>>),
	#[error("expected an ack for a piece of data")]
	ExpectedAck,
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
	#[error("artifact is larger than TFTP supports: {0} chunks of {0} bytes")]
	TooBig(usize, usize),
}

macro_rules! debug_group {
	($github_actions:expr, $($tt:tt)*) => {
		if $github_actions {
			println!("::group::{}", format!($($tt)*));
		} else {
			debug!($($tt)*);
		}
	}
}

macro_rules! debug_group_end {
	($github_actions:expr) => {
		if $github_actions {
			println!("::endgroup::");
		}
	};
}

struct GroupGuard(bool);

impl Drop for GroupGuard {
	fn drop(&mut self) {
		debug_group_end!(self.0);
	}
}

#[async_std::main]
async fn main() -> Result<!, Error> {
	let config = Config::parse();

	if std::env::var("LEVEL").is_err() {
		std::env::set_var("LEVEL", "debug");
	}

	if config.verbose {
		std::env::set_var("LEVEL", "trace");
	}

	pretty_env_logger::try_init_timed_custom_env("LEVEL").expect("failed to initialize logger");

	info!("running Oro Link test session");
	info!("  title:       {}", config.session_name);
	info!("  author:      {}", config.session_author);
	info!("  ref:         {}", config.session_ref);
	info!("  total tests: {}", config.session_num_tests);

	let sock = UnixStream::connect(config.socket_path).await?;
	trace!("connecting to unix stream");

	let receiver = io::BufReader::new(sock.clone());
	let sender = io::BufWriter::new(sock);
	trace!("created buffered reader/writer");

	let (mut sender, mut receiver) = channel::negotiate(
		sender,
		receiver,
		&mut rand::rngs::OsRng,
		ChannelSide::Client,
	)
	.await?;
	trace!("negotiated link channel");

	let mut child_process = process::Command::new(config.cmd[0].clone())
		.args(&config.cmd[1..])
		.reap_on_drop(true)
		.kill_on_drop(true)
		.stdin(process::Stdio::piped())
		.stdout(process::Stdio::piped())
		.stderr(process::Stdio::piped())
		.spawn()?;
	trace!("spawned child process");

	debug!("reporting PXE sizes");
	let size_bios = artifact_size(&config.pxe_dir, &config.pxe_entry_bios).await?;
	debug!("    BIOS bootfile size:   {}", size_bios);

	let size_uefi = artifact_size(&config.pxe_dir, &config.pxe_entry_uefi).await?;
	debug!("    UEFI bootfile size:   {}", size_uefi);

	sender
		.send(Packet::BootfileSize {
			uefi: size_uefi,
			bios: size_bios,
		})
		.await?;
	trace!("sent bootfile sizes");

	info!("starting test session");
	sender
		.send(Packet::StartTestSession {
			total_tests: config.session_num_tests,
			author: config.session_author.as_str().into(),
			title: config.session_name.as_str().into(),
			ref_id: config.session_ref.as_str().into(),
		})
		.await?;
	trace!("sent test session start");

	let mut child_stdin = child_process.stdin.take().unwrap();
	let mut child_stderr = child_process.stderr.take().unwrap();
	trace!("took child process stdin/stderr");

	let mut child_stdout = io::BufReader::new(child_process.stdout.take().unwrap()).lines();
	trace!("took child process stdout");

	#[allow(clippy::large_enum_variant)]
	enum Event {
		ChildStdout(String),
		ChildStderr(usize),
		ChildExit(i32),
		Link(Packet),
	}

	let mut stderr_buf = [0u8; 4096];

	let mut number_passed = 0;
	let mut number_failed = 0;
	let mut number_skipped = 0;

	let mut exit_status;

	trace!("entering event loop");
	loop {
		// We force the event 'queue' to consider the child process exiting first.
		let event = match child_process.try_status()? {
			Some(status) => Event::ChildExit(status.code().unwrap_or(1)),
			None => select! {
				status = child_process.status().fuse() => Event::ChildExit(status?.code().unwrap_or(1)),
				line = child_stdout.next().fuse() => match line {
					Some(Ok(line)) => Event::ChildStdout(line),
					None | Some(Err(_)) => {
						debug!("child process stdout closed");
						continue;
					}
				},
				len = child_stderr.read(&mut stderr_buf).fuse() => match len {
					Ok(len) => Event::ChildStderr(len),
					Err(err) => {
						debug!("child process stderr closed: {:?}", err);
						continue;
					}
				},
				// FIXME(qix-): This is susceptible to stream corruption if the child process performs I/O.
				// FIXME(qix-): We should be putting these things into their own tasks and use channels instead.
				packet = receiver.receive().fuse() => Event::Link(packet?),
			},
		};

		trace!("handling event");

		match event {
			Event::ChildExit(status) => {
				warn!("test process exited with status {status}");
				exit_status = status;
				break;
			}
			Event::ChildStdout(line) => {
				let (first, rest) = line
					.split_once(|c: char| c.is_whitespace())
					.unwrap_or((line.as_str(), ""));

				let (first, rest) = (first.trim(), rest.trim());

				match first {
					"power" => {
						info!("pressing the power button of the machine");
						sender.send(Packet::PressPower).await?;
					}
					"reset" => {
						info!("pressing the reset button of the machine");
						sender.send(Packet::PressReset).await?;
					}
					"echo" => {
						info!("> {rest}");
					}
					"test" => {
						info!("+++ {rest}");
						sender.send(Packet::StartTest { name: rest.into() }).await?;
					}
					"pass" => {
						info!("--- PASS");
						number_passed += 1;
					}
					"fail" => {
						info!("--- FAIL");
						number_failed += 1;
					}
					"skip" => {
						info!("--- SKIP");
						number_skipped += 1;
					}
					unknown => {
						panic!("unknown command from test link: {unknown}");
					}
				}
			}
			Event::ChildStderr(len) => {
				// The 256 comes from the Serial packet heapless vector size.
				// FIXME(qix-): automatically derive the chunk size from the vector size.
				for chunk in stderr_buf[..len].chunks(256) {
					sender
						.send(Packet::Serial(heapless::Vec::from_slice(chunk).unwrap()))
						.await?;
				}
			}
			Event::Link(packet) => match packet {
				Packet::Serial(data) => {
					child_stdin.write_all(&data[..]).await?;
				}
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
							Err(msg)?;
							unreachable!();
						}
						Tftp::OAck(_) => {
							return Err(Error::UnexpectedOack);
						}
						Tftp::Wrq(_) => {
							return Err(Error::UnexpectedWrq);
						}
						Tftp::Rrq(req) => {
							let _group_guard = GroupGuard(config.github_actions);

							let artifact = if req.filename == "ORO_BOOT_UEFI" {
								debug_group!(
									config.github_actions,
									"reading entry point artifact: {} (re-written from ORO_BOOT_UEFI, root: {})",
									config.pxe_entry_uefi,
									config.pxe_dir
								);
								artifact_bytes(&config.pxe_dir, &config.pxe_entry_uefi).await?
							} else if req.filename == "ORO_BOOT_BIOS" {
								debug_group!(
									config.github_actions,
									"reading entry point artifact: {} (re-written from ORO_BOOT_BIOS, root: {})",
									config.pxe_entry_bios,
									config.pxe_dir
								);
								artifact_bytes(&config.pxe_dir, &config.pxe_entry_bios).await?
							} else {
								debug_group!(
									config.github_actions,
									"reading artifact: {} (root: {})",
									req.filename,
									config.pxe_dir
								);
								artifact_bytes(&config.pxe_dir, &req.filename).await?
							};

							let mut opts = req.opts;
							let chunk_size = opts.block_size.unwrap_or(512) as usize;

							if opts.transfer_size.is_some() {
								opts.transfer_size = Some(artifact.len() as u64);
							}

							let opt_ack = Tftp::OAck(opts.clone());
							let buf = heapless::Vec::from_iter(opt_ack.to_bytes());
							sender.send(Packet::Tftp(buf.clone())).await?;
							trace!("sent OACK");

							let should_continue = loop {
								let maybe_oack_ack = select! {
									packet = receiver.receive().fuse() => packet?,
									_ = task::sleep(Duration::from_millis(config.timeout)).fuse() => {
										trace!("resending OACK");
										sender.send(Packet::Tftp(buf.clone())).await?;
										continue;
									}
								};

								if let Packet::Tftp(data) = maybe_oack_ack {
									let packet = async_tftp::parse::parse_packet(data.as_ref())?;
									match packet {
										Tftp::Ack(bid) if bid == 0 => {
											debug!("got oack ack (bid=0); continuing");
											break false;
										}
										Tftp::Error(err)
											if err
												== async_tftp::packet::Error::OptionsNegotiationFailed =>
										{
											debug!(
												"client rejected options; will allow client to re-negotiate"
											);
											break true;
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
							};

							if should_continue {
								continue;
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
										packet = receiver.receive().fuse() => packet?,
										_ = task::sleep(Duration::from_millis(config.timeout)).fuse() => {
											trace!("resending block {i}");
											sender.send(Packet::Tftp(buf.clone())).await?;
											continue;
										}
									};

									match received_packet {
										Packet::Tftp(ack_data) => {
											let packet =
												async_tftp::parse::parse_packet(&ack_data[..]);
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
				unknown => {
					warn!("ignoring unknown packet from link: {unknown:?}");
				}
			},
		}
	}

	let total_reported = number_passed + number_failed + number_skipped;
	let force_error = number_failed > 0 || total_reported != config.session_num_tests;
	if force_error {
		exit_status = 1;
	}

	if total_reported == 0 {
		warn!("no tests were executed!");
		exit_status = 3;
	}

	info!("total passed: {number_passed}");
	info!("total failed: {number_failed}");
	info!("total skipped: {number_skipped}");

	std::process::exit(exit_status);
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
