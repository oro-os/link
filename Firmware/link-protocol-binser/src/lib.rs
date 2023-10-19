#![no_std]

#[cfg(feature = "defmt")]
use defmt::Format;

pub use link_protocol_binser_proc::LinkMessage;

/// Errors that may occur during (de)serialization
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "defmt", derive(Format))]
pub enum Error {
	StringTooLong,
	ArrayTooLong,
	InvalidMessageCode,
	MalformedString,
	Eof,
}

pub trait Serialize {
	fn buffer_length(&self) -> Result<usize, Error>;
	fn serialize_into(&self, buf: &mut [u8]) -> Result<usize, Error>;
}

pub trait Deserialize<'a>
where
	Self: Sized,
{
	fn deserialize(buf: &'a [u8]) -> Result<(Self, usize), Error>;
}

impl Serialize for u8 {
	fn buffer_length(&self) -> Result<usize, Error> {
		Ok(1)
	}

	fn serialize_into(&self, buf: &mut [u8]) -> Result<usize, Error> {
		*buf.first_mut().ok_or(Error::Eof)? = *self;
		self.buffer_length()
	}
}

impl Deserialize<'_> for u8 {
	fn deserialize(buf: &[u8]) -> Result<(Self, usize), Error> {
		Ok((*buf.first().ok_or(Error::Eof)?, 1))
	}
}

impl Serialize for u16 {
	fn buffer_length(&self) -> Result<usize, Error> {
		Ok(2)
	}

	fn serialize_into(&self, buf: &mut [u8]) -> Result<usize, Error> {
		buf.get_mut(..2)
			.ok_or(Error::Eof)?
			.copy_from_slice(&self.to_be_bytes());
		self.buffer_length()
	}
}

impl Deserialize<'_> for u16 {
	fn deserialize(buf: &[u8]) -> Result<(Self, usize), Error> {
		let v = u16::from_be_bytes(buf.get(..2).ok_or(Error::Eof)?.try_into().unwrap());
		Ok((v, 2))
	}
}

impl Serialize for u32 {
	fn buffer_length(&self) -> Result<usize, Error> {
		Ok(4)
	}

	fn serialize_into(&self, buf: &mut [u8]) -> Result<usize, Error> {
		buf.get_mut(..4)
			.ok_or(Error::Eof)?
			.copy_from_slice(&self.to_be_bytes());
		self.buffer_length()
	}
}

impl Deserialize<'_> for u32 {
	fn deserialize(buf: &[u8]) -> Result<(Self, usize), Error> {
		let v = u32::from_be_bytes(buf.get(..4).ok_or(Error::Eof)?.try_into().unwrap());
		Ok((v, 4))
	}
}

impl Serialize for u64 {
	fn buffer_length(&self) -> Result<usize, Error> {
		Ok(8)
	}

	fn serialize_into(&self, buf: &mut [u8]) -> Result<usize, Error> {
		buf.get_mut(..8)
			.ok_or(Error::Eof)?
			.copy_from_slice(&self.to_be_bytes());
		self.buffer_length()
	}
}

impl Deserialize<'_> for u64 {
	fn deserialize(buf: &[u8]) -> Result<(Self, usize), Error> {
		let v = u64::from_be_bytes(buf.get(..8).ok_or(Error::Eof)?.try_into().unwrap());
		Ok((v, 8))
	}
}

impl Serialize for f32 {
	fn buffer_length(&self) -> Result<usize, Error> {
		Ok(4)
	}

	fn serialize_into(&self, buf: &mut [u8]) -> Result<usize, Error> {
		buf.get_mut(..4)
			.ok_or(Error::Eof)?
			.copy_from_slice(&self.to_be_bytes());
		self.buffer_length()
	}
}

impl Deserialize<'_> for f32 {
	fn deserialize(buf: &[u8]) -> Result<(Self, usize), Error> {
		let v = f32::from_be_bytes(buf.get(..4).ok_or(Error::Eof)?.try_into().unwrap());
		Ok((v, 4))
	}
}

impl Serialize for f64 {
	fn buffer_length(&self) -> Result<usize, Error> {
		Ok(8)
	}

	fn serialize_into(&self, buf: &mut [u8]) -> Result<usize, Error> {
		buf.get_mut(..8)
			.ok_or(Error::Eof)?
			.copy_from_slice(&self.to_be_bytes());
		self.buffer_length()
	}
}

impl Deserialize<'_> for f64 {
	fn deserialize(buf: &[u8]) -> Result<(Self, usize), Error> {
		let v = f64::from_be_bytes(buf.get(..8).ok_or(Error::Eof)?.try_into().unwrap());
		Ok((v, 8))
	}
}

impl Serialize for &str {
	fn buffer_length(&self) -> Result<usize, Error> {
		Serialize::buffer_length(&self.as_bytes())
	}

	fn serialize_into(&self, buf: &mut [u8]) -> Result<usize, Error> {
		Serialize::serialize_into(&self.as_bytes(), buf)
	}
}

impl<'a> Deserialize<'a> for &'a str {
	fn deserialize(buf: &'a [u8]) -> Result<(Self, usize), Error> {
		let len = buf.first().ok_or(Error::Eof)?;
		let v = core::str::from_utf8(buf.get(1..(len + 1) as usize).ok_or(Error::Eof)?)
			.map_err(|_| Error::MalformedString)?;
		Ok((v, (len + 1) as usize))
	}
}

impl Serialize for &[u8] {
	fn buffer_length(&self) -> Result<usize, Error> {
		if self.len() > u8::MAX as usize {
			Err(Error::StringTooLong)
		} else {
			Ok(1 + self.len())
		}
	}

	fn serialize_into(&self, buf: &mut [u8]) -> Result<usize, Error> {
		let len = self.len();

		if len > u8::MAX as usize {
			Err(Error::StringTooLong)
		} else {
			*buf.first_mut().ok_or(Error::Eof)? = len as u8;
			buf.get_mut(1..len + 1)
				.ok_or(Error::Eof)?
				.copy_from_slice(self);
			Ok(len + 1)
		}
	}
}

impl<'a> Deserialize<'a> for &'a [u8] {
	fn deserialize(buf: &'a [u8]) -> Result<(Self, usize), Error> {
		let len = buf.first().ok_or(Error::Eof)?;
		Ok((
			buf.get(1..(len + 1) as usize).ok_or(Error::Eof)?,
			(len + 1) as usize,
		))
	}
}

impl<const SZ: usize> Serialize for [u8; SZ] {
	fn buffer_length(&self) -> Result<usize, Error> {
		Ok(SZ)
	}

	fn serialize_into(&self, buf: &mut [u8]) -> Result<usize, Error> {
		buf.get_mut(..SZ)
			.ok_or(Error::Eof)?
			.copy_from_slice(&self[..]);
		Ok(SZ)
	}
}

impl<'a, const SZ: usize> Deserialize<'a> for [u8; SZ] {
	fn deserialize(buf: &'a [u8]) -> Result<(Self, usize), Error> {
		let mut r = [0u8; SZ];
		r.copy_from_slice(buf.get(..SZ).ok_or(Error::Eof)?);
		Ok((r, SZ))
	}
}
