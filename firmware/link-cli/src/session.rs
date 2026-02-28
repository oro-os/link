use std::io::{Read, Write};

use anyhow::{Context, Result, anyhow};
use link_protocol::{
	Request, Response,
	stream::{BUFFER_SIZE, Decoder, encode_request},
};
use static_cell::StaticCell;

const RESET_SENTINEL_LEN: usize = 128;
const RESET_BURN_READS: usize = 65536;

pub struct Session<S> {
	serial:  S,
	decoder: Decoder,
}

impl<S: Read + Write> Session<S> {
	/// Open a synchronized session over `serial`, flushing any in-flight garbage.
	///
	/// # Panics
	/// Panics if called more than once.
	pub fn open(mut serial: S) -> Result<Self> {
		static BUF: StaticCell<[u8; BUFFER_SIZE]> = StaticCell::new();
		let buf = BUF.init([0u8; BUFFER_SIZE]);
		let decoder = Decoder::new(buf);

		// Write garbage + sentinels then drain until we see RESET_SENTINEL_LEN
		// consecutive zero bytes, confirming both sides are in sync.
		serial
			.write_all(&[0xFFu8; 256])
			.context("failed to write garbage bytes to Link")?;
		serial
			.write_all(&[0x00u8; RESET_SENTINEL_LEN])
			.context("failed to write sentinel bytes to Link")?;

		let mut sentinel_count: usize = 0;
		for _ in 0..RESET_BURN_READS {
			let mut byte = [0u8; 1];
			serial
				.read_exact(&mut byte)
				.context("failed to read from Link during sync")?;
			if byte[0] == 0 {
				sentinel_count += 1;
				if sentinel_count >= RESET_SENTINEL_LEN {
					break;
				}
			} else {
				sentinel_count = 0;
			}
		}
		if sentinel_count < RESET_SENTINEL_LEN {
			return Err(anyhow!("timed out waiting for Link to synchronize"));
		}

		Ok(Self { serial, decoder })
	}

	/// Encode and send a single request frame.
	pub fn send(&mut self, request: &Request) -> Result<()> {
		let mut enc_buf = [0u8; BUFFER_SIZE];
		let offlen = encode_request(request, &mut enc_buf)
			.map_err(|e| anyhow!("failed to encode request: {e:?}"))?;
		self.serial
			.write_all(&enc_buf[offlen.offset..offlen.offset + offlen.len])
			.context("failed to write request to serial")
	}

	/// Read bytes from serial until a complete response frame is decoded.
	pub fn recv(&mut self) -> Result<Response> {
		loop {
			let mut byte = [0u8; 1];
			self.serial
				.read_exact(&mut byte)
				.context("failed to read from serial")?;
			let result = self
				.decoder
				.feed(&byte)
				.map_err(|e| anyhow!("decoder feed error: {e:?}"))?;
			if result.is_some() {
				return self
					.decoder
					.decode_response()
					.map_err(|e| anyhow!("failed to decode response: {e:?}"));
			}
		}
	}

	/// Send a request and return the decoded response.
	pub fn request(&mut self, request: &Request) -> Result<Response> {
		self.send(request)?;
		self.recv()
	}
}
