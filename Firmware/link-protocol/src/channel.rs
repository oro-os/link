use crate::{
	macros::{debug, error, info, trace, warn},
	Deserialize, Error, Packet, Read, Serialize, Write,
};
use aes::{
	cipher::{BlockDecrypt, BlockEncrypt, KeyInit},
	Aes256Dec, Aes256Enc,
};
use curve25519::{curve25519, curve25519_pk, curve25519_sk};
use link_protocol_binser::MaybeFormat;
use rand_core::RngCore;

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RWError<R, W>
where
	R: MaybeFormat,
	W: MaybeFormat,
{
	Read(R),
	Write(W),
}

/// Negotiates a connection with a stream reader/writer, forming an
/// encrypted channel and returning a command sender/receiver usable
/// to send and receive AES-encrypted packets.
pub async fn negotiate<W: Write, R: Read, Rng: RngCore>(
	mut sock_writer: W,
	mut sock_reader: R,
	rng: &mut Rng,
) -> Result<(PacketSender<W>, PacketReceiver<R>), RWError<Error<R::Error>, Error<W::Error>>> {
	// Negotiate encryption
	let mut sk = [0u8; 32];
	rng.fill_bytes(&mut sk);
	let sk = curve25519_sk(sk);
	let pk = curve25519_pk(sk);
	let mut their_pk = [0u8; 32];

	// NOTE: It's important to write first, otherwise
	// NOTE: this will deadlock. (:
	sock_writer.write(&pk[..]).await.map_err(RWError::Write)?;

	sock_reader
		.read(&mut their_pk[..])
		.await
		.map_err(RWError::Read)?;

	let key = curve25519(sk, their_pk);
	let enc = Aes256Enc::new_from_slice(&key[..]).unwrap();
	let dec = Aes256Dec::new_from_slice(&key[..]).unwrap();

	Ok((
		PacketSender::new(sock_writer, enc),
		PacketReceiver::new(sock_reader, dec),
	))
}

pub struct PacketSender<W: Write> {
	sock: W,
	tls: Aes256Enc,
	block: [u8; 16],
	cursor: usize,
}

impl<W: Write> PacketSender<W> {
	fn new(sock: W, tls: Aes256Enc) -> Self {
		Self {
			sock,
			tls,
			block: [0; 16],
			cursor: 0,
		}
	}

	pub async fn send(&mut self, packet: Packet) -> Result<(), Error<W::Error>> {
		packet.serialize(self).await?;

		// Flush any remaining contents in the last block
		// Kind of a hack to DRY up the sending sequence...
		if self.cursor > 0 {
			for _ in self.cursor..self.block.len() {
				<Self as Write>::write(self, &[0]).await?;
			}
		}

		Ok(())
	}
}

impl<W: Write> Write for PacketSender<W> {
	type Error = W::Error;

	async fn write(&mut self, buf: &[u8]) -> Result<(), Error<W::Error>> {
		let mut remaining = buf.len();
		let mut off = 0;

		while remaining > 0 {
			debug_assert!(self.cursor <= self.block.len());

			let to_write = remaining.min(self.block.len() - self.cursor);
			self.block[self.cursor..self.cursor + to_write]
				.copy_from_slice(&buf[off..off + to_write]);
			remaining -= to_write;
			off += to_write;
			self.cursor += to_write;

			if self.cursor >= self.block.len() {
				debug_assert_eq!(self.cursor, self.block.len());

				self.tls.encrypt_block((&mut self.block[..]).into());

				self.sock.write(&self.block[..]).await.map_err(|err| {
					error!("daemon: failed to write block: {:?}", err);
					Error::Eof
				})?;

				self.cursor = 0;
			}
		}

		Ok(())
	}
}

pub struct PacketReceiver<R: Read> {
	sock: R,
	tls: Aes256Dec,
	cursor: usize,
	block: [u8; 16],
}

impl<R: Read> PacketReceiver<R> {
	fn new(sock: R, tls: Aes256Dec) -> Self {
		let s = Self {
			sock,
			tls,
			cursor: 16,
			block: [0; 16],
		};

		debug_assert!(s.cursor >= s.block.len());

		s
	}

	pub async fn receive(&mut self) -> Result<Packet, Error<R::Error>> {
		let msg = Packet::deserialize(self).await?;
		self.cursor = self.block.len(); // force a fresh read for the next packet.
		Ok(msg)
	}
}

impl<R: Read> Read for PacketReceiver<R> {
	type Error = R::Error;

	async fn read(&mut self, buf: &mut [u8]) -> Result<(), Error<R::Error>> {
		let mut remaining = buf.len();
		let mut off = 0;

		while remaining > 0 {
			trace!(
				"daemon: read(): remaining={} off={} cursor={}",
				remaining,
				off,
				self.cursor
			);

			if self.cursor >= self.block.len() {
				debug_assert_eq!(self.cursor, self.block.len());

				trace!("daemon: read(): reading 16 bytes from stream");

				self.sock.read(&mut self.block[..]).await.map_err(|err| {
					error!("daemon: failed reading block from socket: {:?}", err);
					Error::Eof
				})?;

				self.cursor = 0;

				trace!("daemon: decrypting bytes from stream");

				self.tls.decrypt_block((&mut self.block[..]).into());
			}

			let to_write = remaining.min(self.block.len() - self.cursor);
			trace!("daemon: to write: {}", to_write);
			buf[off..off + to_write]
				.copy_from_slice(&self.block[self.cursor..self.cursor + to_write]);
			remaining -= to_write;
			off += to_write;
			self.cursor += to_write;
		}

		Ok(())
	}
}
