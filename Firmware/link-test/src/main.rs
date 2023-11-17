#![feature(never_type)]

use async_std::{io, os::unix::net::UnixStream, prelude::*, process};
use clap::Parser;
use futures::{select, FutureExt};
use link_protocol::{
	channel::{self, RWError, Side as ChannelSide},
	Error as PacketError, Packet,
};
use std::process::ExitStatus;

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
	session_num_tests: u64,
	/// The command to run
	#[arg(last = true)]
	cmd: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
enum Error {
	#[error("IO error")]
	Io(#[from] io::Error),
	#[error("link packet error")]
	Packet(#[from] PacketError<io::Error>),
	#[error("IO error during link connection negotiation")]
	RWError(#[from] RWError<PacketError<io::Error>, PacketError<io::Error>>),
	#[error("child process closed an output stream")]
	Eof,
}

#[async_std::main]
async fn main() -> Result<!, Error> {
	let config = Config::parse();

	let sock = UnixStream::connect(config.socket_path).await?;

	let receiver = io::BufReader::new(sock.clone());
	let sender = io::BufWriter::new(sock);

	let (mut sender, mut receiver) = channel::negotiate(
		sender,
		receiver,
		&mut rand::rngs::OsRng,
		ChannelSide::Client,
	)
	.await?;

	let mut child_process = process::Command::new(config.cmd[0].clone())
		.args(&config.cmd[1..])
		.reap_on_drop(true)
		.kill_on_drop(true)
		.stdin(process::Stdio::piped())
		.stdout(process::Stdio::piped())
		.stderr(process::Stdio::piped())
		.spawn()?;

	let mut child_stdin = child_process.stdin.take().unwrap();
	let mut child_stderr = child_process.stderr.take().unwrap();

	let mut child_stdout = io::BufReader::new(child_process.stdout.take().unwrap()).lines();

	let mut stderr_buf = [0u8; 4096];

	#[allow(clippy::large_enum_variant)]
	enum Event {
		ChildStdout(String),
		ChildStderr(usize),
		ChildExit(ExitStatus),
		Link(Packet),
	}

	let mut number_passed = 0;
	let mut number_failed = 0;
	let mut number_skipped = 0;

	let mut exit_status;

	loop {
		let event = select! {
			line = child_stdout.next().fuse() => Event::ChildStdout(line.ok_or(Error::Eof)??),
			len = child_stderr.read(&mut stderr_buf).fuse() => Event::ChildStderr(len?),
			status = child_process.status().fuse() => Event::ChildExit(status?),
			packet = receiver.receive().fuse() => Event::Link(packet?),
		};

		match event {
			Event::ChildExit(status) => {
				println!("test process exited with status {status}");
				exit_status = status.code().unwrap_or(0);
				break;
			}
			Event::ChildStdout(line) => {
				let (first, rest) = line
					.split_once(|c: char| c.is_whitespace())
					.unwrap_or((line.as_str(), ""));

				let (first, rest) = (first.trim(), rest.trim());

				match first {
					"power" => {
						println!("pressing the power button of the machine");
						sender.send(Packet::PressPower).await?;
					}
					"reset" => {
						println!("pressing the reset button of the machine");
						sender.send(Packet::PressReset).await?;
					}
					"echo" => {
						println!("> {rest}");
					}
					"test" => {
						println!("+++ {rest}");
						sender.send(Packet::StartTest { name: rest.into() }).await?;
					}
					"pass" => {
						println!("--- PASS");
						number_passed += 1;
					}
					"fail" => {
						println!("--- FAIL");
						number_failed += 1;
					}
					"skip" => {
						println!("--- SKIP");
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
				unknown => {
					println!("WARNING: ignoring unknown packet from link: {unknown:?}");
				}
			},
		}
	}

	let total_reported = number_passed + number_failed + number_skipped;
	let force_error = number_failed > 0 || total_reported != config.session_num_tests;
	if force_error {
		exit_status = 1;
	}

	println!(
		"\n\ntotal passed: {number_passed}\ntotal failed: {number_failed}\ntotal skipped: {number_skipped}"
	);

	std::process::exit(exit_status);
}
