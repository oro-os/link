use anyhow::Result;
use clap::{Parser, Subcommand};

mod cmd;

#[derive(Debug, Parser)]
#[command(about = "CLI for interacting with Oro Link services")]
struct Cli {
	#[command(subcommand)]
	command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
	/// Generate a self-signed TLS certificate, private key, and public key.
	Keygen(cmd::keygen::KeygenArgs),
}

pub fn main() -> Result<()> {
	match Cli::parse().command {
		Command::Keygen(args) => cmd::keygen::run(args),
	}
}
