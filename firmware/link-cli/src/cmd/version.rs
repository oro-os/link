use std::io::{Read, Write};

use anyhow::{Result, anyhow};
use link_protocol::{Request, Response};

use crate::session::Session;

pub fn run<S: Read + Write>(session: &mut Session<S>) -> Result<()> {
	let major = match session.request(&Request::GetVersionMajor)? {
		Response::Uint(v) => v,
		r => return Err(anyhow!("unexpected response for GetVersionMajor: {r:?}")),
	};

	let minor = match session.request(&Request::GetVersionMinor)? {
		Response::Uint(v) => v,
		r => return Err(anyhow!("unexpected response for GetVersionMinor: {r:?}")),
	};

	let patch = match session.request(&Request::GetVersionPatch)? {
		Response::Uint(v) => v,
		r => return Err(anyhow!("unexpected response for GetVersionPatch: {r:?}")),
	};

	println!("{major}.{minor}.{patch}");
	Ok(())
}
