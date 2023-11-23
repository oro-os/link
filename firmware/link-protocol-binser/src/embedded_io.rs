use crate::{Error, MaybeFormat, Read, Write};
use embedded_io_async::{
	ErrorType, Read as AsyncRead, ReadExactError, Write as AsyncWrite, WriteAllError,
};

impl<T> Read for T
where
	T: AsyncRead,
	<T as ErrorType>::Error: MaybeFormat,
{
	type Error = ReadExactError<<T as ErrorType>::Error>;

	#[inline]
	async fn read(&mut self, buf: &mut [u8]) -> Result<(), Error<Self::Error>> {
		AsyncRead::read_exact(self, buf).await.map_err(Error::Io)
	}
}

impl<T> Write for T
where
	T: AsyncWrite,
	<T as ErrorType>::Error: MaybeFormat,
{
	type Error = <T as ErrorType>::Error;

	#[inline]
	async fn write(&mut self, buf: &[u8]) -> Result<(), Error<Self::Error>> {
		AsyncWrite::write_all(self, buf)
			.await
			.map_err(|err| match err {
				// Probably not the best analog but this should work.
				WriteAllError::WriteZero => Error::Eof,
				WriteAllError::Other(err) => Error::Io(err),
			})
	}

	#[inline]
	async fn flush(&mut self) -> Result<(), Self::Error> {
		AsyncWrite::flush(self).await
	}
}
