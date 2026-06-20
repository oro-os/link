//! Redis client implementation for the Link firmware.
use core::{
	fmt::{Display, Write as _},
	unreachable,
};

use embassy_net::tcp::TcpSocket;
use embedded_io_async::{Read, Write};

pub type Result<T, E = Error> = core::result::Result<T, E>;

#[derive(Debug, defmt::Format, Clone, PartialEq)]
pub enum Error {
	Tcp(embassy_net::tcp::Error),
	ReadExact(embedded_io_async::ReadExactError<embassy_net::tcp::Error>),
	UnexpectedResponse,
	ProtocolError,
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

		let response = Response::parse(&mut self.sock, self.buf).await?;
		let s = match response {
			Response::BulkString(Some(s)) => s,
			Response::SimpleString(s) => s,
			other => {
				defmt::error!(
					"redis responded to PING with invalid response type: {:?}",
					other
				);
				return Err(Error::ProtocolError);
			}
		};

		match s.parse::<u32>() {
			Ok(p) if p == rn => Ok(()),
			Ok(p) => {
				defmt::error!(
					"redis responded to PING with a different pong value: {:?}",
					p
				);
				Err(Error::ProtocolError)
			}
			Err(_) => {
				defmt::error!("redis sent unknown PING response message: {:?}", s);
				Err(Error::ProtocolError)
			}
		}
	}

	pub async fn set(&mut self, key: impl Display, value: impl Display) -> Result<()> {
		RedisWriter::start(self.buf, "SET", 2)?
			.arg(format_args!("{}{key}", self.prefix))?
			.arg(value)?
			.finish(&mut self.sock)
			.await?;

		let response = Response::parse(&mut self.sock, self.buf).await?;
		match response {
			Response::SimpleString(s) if s == "OK" => Ok(()),
			other => {
				defmt::error!("redis responded to SET with invalid response: {:?}", other);
				Err(Error::ProtocolError)
			}
		}
	}

	pub async fn get<R: core::str::FromStr>(&mut self, key: impl Display) -> Result<Option<R>> {
		RedisWriter::start(self.buf, "GET", 1)?
			.arg(format_args!("{}{key}", self.prefix))?
			.finish(&mut self.sock)
			.await?;

		let response = Response::parse(&mut self.sock, self.buf).await?;
		let s = match response {
			Response::BulkString(Some(s)) => s,
			Response::SimpleString(s) => s,
			Response::BulkString(None) => return Ok(None),
			other => {
				defmt::error!(
					"redis responded to GET with invalid response type: {:?}",
					other
				);
				return Err(Error::ProtocolError);
			}
		};
		let r = s.parse().map(Some).map_err(|_| {
			defmt::error!("redis responded to GET with invalid value: {:?}", s);
			Error::UnexpectedResponse
		})?;
		Ok(r)
	}

	pub async fn del(&mut self, key: impl Display) -> Result<bool> {
		RedisWriter::start(self.buf, "DEL", 1)?
			.arg(format_args!("{}{key}", self.prefix))?
			.finish(&mut self.sock)
			.await?;

		let response = Response::parse(&mut self.sock, self.buf).await?;
		match response {
			Response::Integer(0) => Ok(false),
			Response::Integer(_) => Ok(true),
			other => {
				defmt::error!(
					"redis responded to DEL with invalid response type: {:?}",
					other
				);
				Err(Error::ProtocolError)
			}
		}
	}
}

#[derive(defmt::Format)]
enum Response<'a> {
	SimpleString(&'a str),
	BulkString(Option<&'a str>),
	SimpleError(&'a str),
	Integer(i64),
	Null,
	Boolean(bool),
	Double(f64),
	BulkError(&'a str),
}

impl<'a> Response<'a> {
	async fn parse<const N: usize>(
		sock: &'a mut TcpSocket<'_>,
		buf: &'a mut heapless::Vec<u8, N>,
	) -> Result<Self> {
		buf.resize(1, 0).map_err(|_| Error::TooLong)?;
		sock.read_exact(&mut buf[..1]).await?;
		match buf[0] {
			b'+' => Self::parse_simple_string(sock, buf).await,
			b'-' => Self::parse_simple_error(sock, buf).await,
			b':' => Self::parse_integer(sock, buf).await,
			b'$' => Self::parse_bulk_string(sock, buf).await,
			// b'*' => Self::burn_array(sock, buf).await,
			b'_' => Self::parse_null(sock, buf).await,
			b'#' => Self::parse_boolean(sock, buf).await,
			b',' => Self::parse_double(sock, buf).await,
			// b'(' => Self::burn_big_number(sock, buf).await,
			b'!' => Self::parse_bulk_error(sock, buf).await,
			// b'=' => Self::parse_verbatim_string(sock, buf).await,
			// b'%' => Self::burn_map(sock, buf).await,
			// b'|' => Self::burn_attribute(sock, buf).await,
			// b'~' => Self::burn_set(sock, buf).await,
			// b'>' => Self::burn_push(sock, buf).await,
			u => {
				defmt::error!("unexpected response type: {:02X}", u);
				Err(Error::ProtocolError)
			}
		}
	}

	async fn read_line<'b, const N: usize>(
		sock: &'_ mut TcpSocket<'_>,
		buf: &'b mut heapless::Vec<u8, N>,
	) -> Result<&'b str> {
		buf.resize(buf.capacity(), 0).unwrap();
		let mut pos = 0;
		while pos < buf.len() {
			sock.read_exact(&mut buf[pos..pos + 1]).await?;

			if buf[pos] == b'\r' {
				if (pos + 1) >= buf.len() {
					return Err(Error::TooLong);
				}

				sock.read_exact(&mut buf[pos + 1..pos + 2]).await?;
				if buf[pos + 1] == b'\n' {
					let line =
						core::str::from_utf8(&buf[..pos]).map_err(|_| Error::ProtocolError)?;
					return Ok(line);
				}
				pos += 1;
			}

			pos += 1;
		}

		Err(Error::TooLong)
	}

	async fn parse_simple_string<const N: usize>(
		sock: &'_ mut TcpSocket<'_>,
		buf: &'a mut heapless::Vec<u8, N>,
	) -> Result<Self> {
		let s = Self::read_line(sock, buf).await?;
		Ok(Self::SimpleString(s))
	}

	async fn parse_simple_error<const N: usize>(
		sock: &'_ mut TcpSocket<'_>,
		buf: &'a mut heapless::Vec<u8, N>,
	) -> Result<Self> {
		let s = Self::read_line(sock, buf).await?;
		Ok(Self::SimpleError(s))
	}

	async fn parse_integer<const N: usize>(
		sock: &'_ mut TcpSocket<'_>,
		buf: &'a mut heapless::Vec<u8, N>,
	) -> Result<Self> {
		let s = Self::read_line(sock, buf).await?;
		let i = s.parse::<i64>().map_err(|_| {
			defmt::error!("invalid integer: {}", s);
			Error::ProtocolError
		})?;
		Ok(Self::Integer(i))
	}

	async fn parse_bulk_string<const N: usize>(
		sock: &'_ mut TcpSocket<'_>,
		buf: &'a mut heapless::Vec<u8, N>,
	) -> Result<Self> {
		let s = Self::read_line(sock, buf).await?;
		if s == "-1" {
			return Ok(Self::BulkString(None));
		}
		let len = s.parse::<usize>().map_err(|_| {
			defmt::error!("invalid bulk string length: {}", s);
			Error::ProtocolError
		})?;
		buf.resize(len, 0).map_err(|_| Error::TooLong)?;
		sock.read_exact(&mut buf[..len]).await?;
		let s = core::str::from_utf8(&buf[..len]).map_err(|_| {
			defmt::error!("invalid UTF-8 in bulk string: {:X}", buf[..len]);
			Error::ProtocolError
		})?;
		// Read the trailing \r\n
		let mut rnbuf = [0u8; 2];
		sock.read_exact(&mut rnbuf[..]).await?;
		if &rnbuf != b"\r\n" {
			defmt::error!(
				"expected CRLF after bulk string data, got {=u8:X} {=u8:X}",
				rnbuf[0],
				rnbuf[1]
			);
			return Err(Error::ProtocolError);
		}
		Ok(Self::BulkString(Some(s)))
	}

	async fn parse_null<const N: usize>(
		sock: &'_ mut TcpSocket<'_>,
		_buf: &'a mut heapless::Vec<u8, N>,
	) -> Result<Self> {
		// Read the trailing \r\n
		let mut crlf = [0u8; 2];
		sock.read_exact(&mut crlf).await?;
		if &crlf != b"\r\n" {
			defmt::error!(
				"expected CRLF after null, got {=u8:X} {=u8:X}",
				crlf[0],
				crlf[1]
			);
			return Err(Error::ProtocolError);
		}
		Ok(Self::Null)
	}

	async fn parse_boolean<const N: usize>(
		sock: &'_ mut TcpSocket<'_>,
		_buf: &'a mut heapless::Vec<u8, N>,
	) -> Result<Self> {
		let mut b = [0u8; 3];
		sock.read_exact(&mut b).await?;
		match &b {
			b"t\r\n" => Ok(Self::Boolean(true)),
			b"f\r\n" => Ok(Self::Boolean(false)),
			_ => {
				defmt::error!("invalid boolean value: {:X}", b);
				Err(Error::ProtocolError)
			}
		}
	}

	async fn parse_double<const N: usize>(
		sock: &'_ mut TcpSocket<'_>,
		buf: &'a mut heapless::Vec<u8, N>,
	) -> Result<Self> {
		let s = Self::read_line(sock, buf).await?;
		let f = s.parse::<f64>().map_err(|_| {
			defmt::error!("invalid double: {}", s);
			Error::ProtocolError
		})?;
		Ok(Self::Double(f))
	}

	async fn parse_bulk_error<const N: usize>(
		sock: &'_ mut TcpSocket<'_>,
		buf: &'a mut heapless::Vec<u8, N>,
	) -> Result<Self> {
		let Self::BulkString(s) = Self::parse_bulk_string(sock, buf).await? else {
			unreachable!();
		};
		Ok(Self::BulkError(s.unwrap_or("")))
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

	fn arg(self, arg: impl Display) -> Result<Self> {
		core::write!(self.buf, "${}\r\n{}\r\n", WriteCounter::new(&arg), arg)
			.map_err(|_| Error::TooLong)?;
		Ok(self)
	}

	async fn finish<'b>(self, sock: &'_ mut TcpSocket<'b>) -> Result<()> {
		sock.write_all(self.buf).await?;
		Ok(())
	}
}
