#![no_std]
#![feature(async_fn_in_trait)]

#[cfg(feature = "defmt")]
use defmt::Format;

pub use link_protocol_binser_proc::LinkMessage;

#[cfg(feature = "defmt")]
pub trait MaybeFormat: defmt::Format + core::fmt::Debug {}
#[cfg(feature = "defmt")]
impl<T> MaybeFormat for T where T: defmt::Format + core::fmt::Debug {}

#[cfg(not(feature = "defmt"))]
pub trait MaybeFormat: core::fmt::Debug {}
#[cfg(not(feature = "defmt"))]
impl<T> MaybeFormat for T where T: core::fmt::Debug {}

/// Errors that may occur during (de)serialization
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "defmt", derive(Format))]
pub enum Error<IoError: MaybeFormat> {
	StringTooLong,
	ArrayTooLong,
	InvalidMessageCode,
	MalformedString,
	Eof,
	Io(IoError),
}

impl<E: MaybeFormat> From<E> for Error<E> {
	fn from(value: E) -> Self {
		Self::Io(value)
	}
}

pub trait Write {
	type Error: MaybeFormat;

	/// Write the entirety of `buf` to the stream.
	async fn write(&mut self, buf: &[u8]) -> Result<(), Error<Self::Error>>;
}

pub trait Read {
	type Error: MaybeFormat;

	/// Read exactly `buf.len()` bytes into `buf`.
	async fn read(&mut self, buf: &mut [u8]) -> Result<(), Error<Self::Error>>;
}

pub trait Serialize {
	async fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), Error<W::Error>>;
}

pub trait Deserialize
where
	Self: Sized,
{
	async fn deserialize<R: Read>(reader: &mut R) -> Result<Self, Error<R::Error>>;
}

impl Serialize for u8 {
	async fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), Error<W::Error>> {
		writer.write(&[*self]).await
	}
}

impl Deserialize for u8 {
	async fn deserialize<R: Read>(reader: &mut R) -> Result<Self, Error<R::Error>> {
		let mut buf = [0u8; 1];
		reader.read(&mut buf).await?;
		Ok(buf[0])
	}
}

impl Serialize for u16 {
	async fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), Error<W::Error>> {
		let bytes = self.to_be_bytes();
		writer.write(&bytes).await
	}
}

impl Deserialize for u16 {
	async fn deserialize<R: Read>(reader: &mut R) -> Result<Self, Error<R::Error>> {
		let mut buf = [0u8; 2];
		reader.read(&mut buf).await?;
		let value = u16::from_be_bytes(buf);
		Ok(value)
	}
}

impl Serialize for u32 {
	async fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), Error<W::Error>> {
		let bytes = self.to_be_bytes();
		writer.write(&bytes).await
	}
}

impl Deserialize for u32 {
	async fn deserialize<R: Read>(reader: &mut R) -> Result<Self, Error<R::Error>> {
		let mut buf = [0u8; 4];
		reader.read(&mut buf).await?;
		let value = u32::from_be_bytes(buf);
		Ok(value)
	}
}

impl Serialize for u64 {
	async fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), Error<W::Error>> {
		let bytes = self.to_be_bytes();
		writer.write(&bytes).await
	}
}

impl Deserialize for u64 {
	async fn deserialize<R: Read>(reader: &mut R) -> Result<Self, Error<R::Error>> {
		let mut buf = [0u8; 8];
		reader.read(&mut buf).await?;
		let value = u64::from_be_bytes(buf);
		Ok(value)
	}
}

impl Serialize for f32 {
	async fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), Error<W::Error>> {
		let bytes = self.to_be_bytes();
		writer.write(&bytes).await
	}
}

impl Deserialize for f32 {
	async fn deserialize<R: Read>(reader: &mut R) -> Result<Self, Error<R::Error>> {
		let mut buf = [0u8; 4];
		reader.read(&mut buf).await?;
		let value = f32::from_be_bytes(buf);
		Ok(value)
	}
}

impl Serialize for f64 {
	async fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), Error<W::Error>> {
		let bytes = self.to_be_bytes();
		writer.write(&bytes).await
	}
}

impl Deserialize for f64 {
	async fn deserialize<R: Read>(reader: &mut R) -> Result<Self, Error<R::Error>> {
		let mut buf = [0u8; 8];
		reader.read(&mut buf).await?;
		let value = f64::from_be_bytes(buf);
		Ok(value)
	}
}

impl<const SZ: usize> Serialize for [u8; SZ] {
	async fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), Error<W::Error>> {
		writer.write(&self[..]).await?;
		Ok(())
	}
}

impl<const SZ: usize> Deserialize for [u8; SZ] {
	async fn deserialize<R: Read>(reader: &mut R) -> Result<Self, Error<R::Error>> {
		let mut r = [0u8; SZ];
		reader.read(&mut r).await?;
		Ok(r)
	}
}

#[cfg(feature = "heapless")]
const fn num_bytes_for_size<const SZ: usize>() -> usize {
	const U8_MAX: usize = u8::MAX as usize;
	const U8_UPPER: usize = (u8::MAX as usize) + 1;
	const U16_MAX: usize = u16::MAX as usize;

	match SZ {
		0..=U8_MAX => 1,
		U8_UPPER..=U16_MAX => 2,
		_ => 4,
	}
}

#[cfg(feature = "heapless")]
impl<const SZ: usize> Serialize for heapless::String<SZ> {
	async fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), Error<W::Error>> {
		let bytes = self.as_bytes();
		let len = bytes.len();

		debug_assert!(len <= SZ);
		debug_assert!(len <= u32::MAX as usize);

		let num_bytes = num_bytes_for_size::<SZ>();

		let len_bytes = (len as u32).to_be_bytes();

		writer.write(&len_bytes[(4 - num_bytes)..]).await?;
		writer.write(bytes).await
	}
}

#[cfg(feature = "heapless")]
impl<const SZ: usize> Deserialize for heapless::String<SZ> {
	async fn deserialize<R: Read>(reader: &mut R) -> Result<Self, Error<R::Error>> {
		let num_bytes = num_bytes_for_size::<SZ>();

		let mut len_bytes = [0u8; 4];
		reader.read(&mut len_bytes[4 - num_bytes..]).await?;

		let len = u32::from_be_bytes(len_bytes) as usize;

		if len > SZ {
			return Err(Error::StringTooLong);
		}

		let mut buffer = [0u8; SZ];
		reader.read(&mut buffer[..len]).await?;

		let utf8 = core::str::from_utf8(&buffer[0..len]).map_err(|_| Error::MalformedString)?;
		Ok(heapless::String::from(utf8))
	}
}
