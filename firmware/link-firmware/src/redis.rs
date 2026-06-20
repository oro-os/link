//! Redis client implementation for the Link firmware.
use core::fmt::{Display, Write as _};

use embassy_net::tcp::TcpSocket;
use embedded_io_async::{Read, Write};

pub type Result<T, E = Error> = core::result::Result<T, E>;

#[derive(Debug, defmt::Format, Clone, PartialEq)]
pub enum Error {
	Tcp(embassy_net::tcp::Error),
	ReadExact(embedded_io_async::ReadExactError<embassy_net::tcp::Error>),
	UnexpectedResponse,
	TooLong,
}

impl From<embassy_net::tcp::Error> for Error {
	#[inline]
	fn from(e: embassy_net::tcp::Error) -> Self {
		Error::Tcp(e)
	}
}

impl From<embedded_io_async::ReadExactError<embassy_net::tcp::Error>> for Error {
	#[inline]
	fn from(e: embedded_io_async::ReadExactError<embassy_net::tcp::Error>) -> Self {
		Error::ReadExact(e)
	}
}

pub struct Client<'a, const N: usize> {
	sock:   TcpSocket<'a>,
	buf:    &'a mut heapless::Vec<u8, N>,
	prefix: &'a str,
}

impl<'a, const N: usize> Client<'a, N> {
	pub fn new(sock: TcpSocket<'a>, buf: &'a mut heapless::Vec<u8, N>, prefix: &'a str) -> Self {
		Self { sock, buf, prefix }
	}

	pub async fn ping(&mut self) -> Result<()> {
		let rn = crate::rand::next_u32();
		RedisWriter::start(self.buf, "PING", 1)?
			.arg(rn)?
			.finish(&mut self.sock)
			.await?;

		let expected = heapless::format!(64; "${}\r\n{}\r\n", WriteCounter::new(&rn), rn).unwrap();
		self.buf
			.resize(expected.len(), 0)
			.map_err(|_| Error::TooLong)?;
		self.sock
			.read_exact(&mut self.buf[..expected.len()])
			.await?;
		if &self.buf[..expected.len()] == expected.as_bytes() {
			Ok(())
		} else {
			Err(Error::UnexpectedResponse)
		}
	}

	pub async fn set(&mut self, key: impl Display, value: impl Display) -> Result<()> {
		RedisWriter::start(self.buf, "SET", 2)?
			.arg(format_args!("{}{key}", self.prefix))?
			.arg(value)?
			.finish(&mut self.sock)
			.await?;
		let expected = b"+OK\r\n";
		self.buf
			.resize(expected.len(), 0)
			.map_err(|_| Error::TooLong)?;
		self.sock
			.read_exact(&mut self.buf[..expected.len()])
			.await?;
		if &self.buf[..expected.len()] == expected {
			Ok(())
		} else {
			Err(Error::UnexpectedResponse)
		}
	}

	pub async fn get<R: core::str::FromStr>(&mut self, key: impl Display) -> Result<Option<R>> {
		RedisWriter::start(self.buf, "GET", 1)?
			.arg(format_args!("{}{key}", self.prefix))?
			.finish(&mut self.sock)
			.await?;
		self.buf.resize(1, 0).map_err(|_| Error::TooLong)?;
		self.sock.read_exact(&mut self.buf[..1]).await?;
		match self.buf[0] {
			b'$' => {
				let Some(len) = self.read_length().await? else {
					return Ok(None);
				};
				self.buf.resize(len, 0).map_err(|_| Error::TooLong)?;
				self.sock.read_exact(&mut self.buf[..len]).await?;
				let s = core::str::from_utf8(&self.buf[..len])
					.map_err(|_| Error::UnexpectedResponse)?;
				let r = s.parse().map(Some).map_err(|_| Error::UnexpectedResponse)?;
				// Read the trailing \r\n
				self.buf.resize(2, 0).map_err(|_| Error::TooLong)?;
				self.sock.read_exact(&mut self.buf[..2]).await?;
				Ok(r)
			}
			_ => Err(Error::UnexpectedResponse),
		}
	}

	async fn read_length(&mut self) -> Result<Option<usize>> {
		let mut len_buf = [0u8; 16];
		let mut pos = 0;
		loop {
			self.sock.read_exact(&mut len_buf[pos..pos + 1]).await?;
			if len_buf[pos] == b'\r' {
				self.sock.read_exact(&mut len_buf[pos + 1..pos + 2]).await?;
				if len_buf[pos + 1] == b'\n' {
					break;
				} else {
					return Err(Error::UnexpectedResponse);
				}
			}
			pos += 1;
			if pos >= len_buf.len() {
				return Err(Error::TooLong);
			}
		}
		if len_buf[0] == b'-' {
			return Ok(None);
		}
		let len_str =
			core::str::from_utf8(&len_buf[..pos]).map_err(|_| Error::UnexpectedResponse)?;
		let len = len_str
			.parse::<usize>()
			.map_err(|_| Error::UnexpectedResponse)?;
		Ok(Some(len))
	}
}

struct WriteCounter {
	count: usize,
}

impl WriteCounter {
	fn new(v: &impl Display) -> Self {
		let mut s = Self { count: 0 };
		core::write!(&mut s, "{}", v).unwrap();
		s
	}
}

impl core::fmt::Write for WriteCounter {
	fn write_char(&mut self, c: char) -> core::fmt::Result {
		self.count += c.len_utf8();
		Ok(())
	}

	fn write_str(&mut self, s: &str) -> core::fmt::Result {
		self.count += s.len();
		Ok(())
	}
}

impl core::fmt::Display for WriteCounter {
	#[inline]
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		self.count.fmt(f)
	}
}

struct RedisWriter<'a, const N: usize> {
	buf: &'a mut heapless::Vec<u8, N>,
}

impl<'a, const N: usize> RedisWriter<'a, N> {
	fn start(buf: &'a mut heapless::Vec<u8, N>, cmd: &'static str, nargs: usize) -> Result<Self> {
		buf.clear();
		core::write!(buf, "*{}\r\n${}\r\n{}\r\n", 1 + nargs, cmd.len(), cmd)
			.map_err(|_| Error::TooLong)?;
		Ok(Self { buf })
	}

	fn arg(mut self, arg: impl Display) -> Result<Self> {
		core::write!(self.buf, "${}\r\n{}\r\n", WriteCounter::new(&arg), arg)
			.map_err(|_| Error::TooLong)?;
		Ok(self)
	}

	async fn finish<'b>(self, sock: &'_ mut TcpSocket<'b>) -> Result<()> {
		sock.write_all(self.buf).await?;
		Ok(())
	}
}
