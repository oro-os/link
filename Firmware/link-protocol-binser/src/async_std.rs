use crate::{Error, Read, Write};
use async_std::io::{Error as AsyncError, ReadExt as AsyncRead, WriteExt as AsyncWrite};

impl<T> Read for T
where
	T: AsyncRead + Unpin,
{
	type Error = AsyncError;

	#[inline]
	async fn read<'a>(&'a mut self, buf: &mut [u8]) -> Result<(), Error<Self::Error>> {
		AsyncRead::read_exact(self, buf).await.map_err(Error::Io)
	}
}

impl<T> Write for T
where
	T: AsyncWrite + Unpin,
{
	type Error = AsyncError;

	#[inline]
	async fn write<'a>(&'a mut self, buf: &[u8]) -> Result<(), Error<Self::Error>> {
		AsyncWrite::write_all(self, buf).await.map_err(Error::Io)
	}

	#[inline]
	async fn flush(&mut self) -> Result<(), Self::Error> {
		AsyncWrite::flush(self).await
	}
}
