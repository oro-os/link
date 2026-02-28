mod cmd;
mod session;

use anyhow::Context;
use clap::Parser;

#[derive(Debug, Parser)]
struct Args {
	/// The serial device to use
	#[clap(short = 'D', long = "device")]
	serial_device: String,
	/// The subcommand to run
	#[clap(subcommand)]
	command:       Cmd,
}

#[derive(Debug, Parser, Clone)]
enum Cmd {
	/// Gets the version of the Link
	#[clap(name = "version")]
	Version,
}

pub fn main() -> Result<(), Box<dyn core::error::Error>> {
	let args = Args::parse();

	let serial = serial::open(&args.serial_device).context(format!(
		"failed to open serial device: {}",
		args.serial_device
	))?;

	let mut session = session::Session::open(serial)?;

	match args.command {
		Cmd::Version => Ok(cmd::version::run(&mut session)?),
	}
}
