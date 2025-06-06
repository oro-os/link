use crate::{
	Deserialize, Error, Packet, Read, Serialize, Write,
	macros::{debug, error, trace},
};
use aes::{
	Aes256Dec, Aes256Enc,
	cipher::{BlockDecrypt, BlockEncrypt, KeyInit},
};
#[cfg(feature = "async-std")]
use async_std::sync::Mutex;
use core::ops::DerefMut;
use curve25519::{curve25519, curve25519_pk, curve25519_sk};
use link_protocol_binser::MaybeFormat;
use rand_core::RngCore;
#[cfg(feature = "embassy")]
type Mutex<T> = ::embassy_sync::mutex::Mutex<::embassy_sync::blocking_mutex::raw::NoopRawMutex, T>;

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg_attr(feature = "thiserror", derive(::thiserror::Error))]
pub enum RWError<R, W>
where
	R: MaybeFormat,
	W: MaybeFormat,
{
	#[cfg_attr(
		feature = "thiserror",
		error("an error occurred reading from the peer: {0}")
	)]
	Read(R),
	#[cfg_attr(
		feature = "thiserror",
		error("an error occurred writing to the peer: {0}")
	)]
	Write(W),
}

pub enum Side {
	Client,
	Server,
}

/// Negotiates a connection with a stream reader/writer, forming an
/// encrypted channel and returning a command sender/receiver usable
/// to send and receive AES-encrypted packets.
pub async fn negotiate<W: Write, R: Read, Rng: RngCore>(
	mut sock_writer: W,
	mut sock_reader: R,
	rng: &mut Rng,
	side: Side,
) -> Result<(PacketSender<W>, PacketReceiver<R>), RWError<Error<R::Error>, Error<W::Error>>> {
	debug!("link-proto: beginning encryption negotiation");

	let mut sk = [0u8; 32];
	let mut their_pk = [0u8; 32];

	rng.fill_bytes(&mut sk);
	let sk = curve25519_sk(sk);
	let pk = curve25519_pk(sk);

	debug!("link-proto: generated secret key");

	let side_order = match side {
		Side::Client => [Side::Client, Side::Server],
		Side::Server => [Side::Server, Side::Client],
	};

	for side in side_order {
		match side {
			Side::Client => {
				debug!("link-proto: writing public key");
				sock_writer.write(&pk[..]).await.map_err(RWError::Write)?;
				sock_writer
					.flush()
					.await
					.map_err(|err| RWError::Write(Error::Io(err)))?;
				debug!("link-proto: wrote public key");
			}
			Side::Server => {
				debug!("link-proto: reading public key");
				sock_reader
					.read(&mut their_pk[..])
					.await
					.map_err(RWError::Read)?;
				debug!("link-proto: read public key");
			}
		}
	}

	let key = curve25519(sk, their_pk);
	debug!("link-proto: generated shared key");

	let enc = Aes256Enc::new_from_slice(&key[..]).unwrap();
	let dec = Aes256Dec::new_from_slice(&key[..]).unwrap();
	debug!("link-proto: generated encryption instances");

	Ok((
		PacketSender::new(sock_writer, enc),
		PacketReceiver::new(sock_reader, dec),
	))
}

pub struct PacketSender<W: Write> {
	sock: Mutex<BlockSender<W>>,
}

impl<W: Write> PacketSender<W> {
	fn new(sock: W, tls: Aes256Enc) -> Self {
		Self {
			sock: Mutex::new(BlockSender {
				sock,
				tls,
				block: [0; 16],
				cursor: 0,
			}),
		}
	}

	pub async fn send(&mut self, packet: Packet) -> Result<(), Error<W::Error>> {
		let mut sock = self.sock.lock().await;
		packet.serialize(sock.deref_mut()).await?;
		sock.flush().await
	}
}

struct BlockSender<W: Write> {
	sock: W,
	tls: Aes256Enc,
	block: [u8; 16],
	cursor: usize,
}

impl<W: Write> BlockSender<W> {
	async fn flush(&mut self) -> Result<(), Error<W::Error>> {
		// Flush any remaining contents in the last block
		// Kind of a hack to DRY up the sending sequence...
		if self.cursor > 0 {
			for _ in self.cursor..self.block.len() {
				<Self as Write>::write(self, &[0]).await?;
			}
		}

		Ok(self.sock.flush().await?)
	}
}

impl<W: Write> Write for BlockSender<W> {
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

				self.sock
					.write(&self.block[..])
					.await
					.map_err(|#[allow(unused)] err| {
						error!("link-proto: failed to write block: {:?}", err);
						Error::Eof
					})?;

				self.cursor = 0;
			}
		}

		Ok(())
	}

	#[inline]
	async fn flush(&mut self) -> Result<(), Self::Error> {
		W::flush(&mut self.sock).await
	}
}

pub struct PacketReceiver<R: Read> {
	sock: Mutex<BlockReceiver<R>>,
}

impl<R: Read> PacketReceiver<R> {
	fn new(sock: R, tls: Aes256Dec) -> Self {
		Self {
			sock: Mutex::new(BlockReceiver {
				sock,
				tls,
				cursor: 16, // must start as >= the block size!
				block: [0; 16],
			}),
		}
	}

	pub async fn receive(&mut self) -> Result<Packet, Error<R::Error>> {
		let mut sock = self.sock.lock().await;
		let msg = Packet::deserialize(sock.deref_mut()).await?;
		sock.cursor = sock.block.len(); // force a fresh read for the next packet.
		Ok(msg)
	}
}

struct BlockReceiver<R: Read> {
	sock: R,
	tls: Aes256Dec,
	cursor: usize,
	block: [u8; 16],
}

impl<R: Read> Read for BlockReceiver<R> {
	type Error = R::Error;

	async fn read(&mut self, buf: &mut [u8]) -> Result<(), Error<R::Error>> {
		let mut remaining = buf.len();
		let mut off = 0;

		while remaining > 0 {
			trace!(
				"link-proto: read(): remaining={} off={} cursor={}",
				remaining, off, self.cursor
			);

			if self.cursor >= self.block.len() {
				debug_assert_eq!(self.cursor, self.block.len());

				trace!("link-proto: read(): reading 16 bytes from stream");

				self.sock
					.read(&mut self.block[..])
					.await
					.map_err(|#[allow(unused)] err| {
						error!("link-proto: failed reading block from socket: {:?}", err);
						Error::Eof
					})?;

				self.cursor = 0;

				trace!("link-proto: decrypting bytes from stream");

				self.tls.decrypt_block((&mut self.block[..]).into());
			}

			let to_write = remaining.min(self.block.len() - self.cursor);
			trace!("link-proto: to write: {}", to_write);
			buf[off..off + to_write]
				.copy_from_slice(&self.block[self.cursor..self.cursor + to_write]);
			remaining -= to_write;
			off += to_write;
			self.cursor += to_write;
		}

		Ok(())
	}
}
