#[cfg(feature = "typescript")]
use wasm_bindgen::prelude::*;

// Since minicbor doesn't give us static buffer sizes, we have to assume that 8x the request length
// is enough. We do check for this and error handle properly but it's a good enough guess to avoid
// length issues.
pub const PACKET_SIZE: usize = size_of::<crate::Request>().max(size_of::<crate::Response>()) * 8;
pub const BUFFER_SIZE: usize = cobs::max_encoding_length(PACKET_SIZE) * 2;

#[cfg(feature = "typescript")]
#[wasm_bindgen]
pub fn packet_size() -> usize {
	PACKET_SIZE
}

#[cfg(feature = "typescript")]
#[wasm_bindgen]
pub fn buffer_size() -> usize {
	BUFFER_SIZE
}

#[cfg_attr(feature = "typescript", wasm_bindgen)]
pub struct Decoder {
	decoder:      cobs::CobsDecoder<'static>,
	decoded_size: Option<usize>,
}

#[cfg_attr(feature = "typescript", wasm_bindgen)]
pub struct DecodeResult {
	pub decoded_size: usize,
	pub leftover:     usize,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg_attr(feature = "typescript", wasm_bindgen)]
#[derive(Debug, Clone, Copy)]
pub enum StreamError {
	Empty,
	Invalid,
	Incomplete,
	Completed,
	TooMuchData,
}

impl From<cobs::DecodeError> for StreamError {
	fn from(v: cobs::DecodeError) -> Self {
		match v {
			cobs::DecodeError::EmptyFrame => Self::Empty,
			cobs::DecodeError::TargetBufTooSmall => Self::TooMuchData,
			cobs::DecodeError::InvalidFrame { .. } => Self::Invalid,
		}
	}
}

impl From<minicbor::decode::Error> for StreamError {
	fn from(_: minicbor::decode::Error) -> Self {
		Self::Invalid
	}
}

impl<E> From<minicbor::encode::Error<E>> for StreamError {
	fn from(_: minicbor::encode::Error<E>) -> Self {
		// We of take a guess here; there can really only be one thing
		// that goes wrong here, as per our implementation.
		Self::TooMuchData
	}
}

impl From<cobs::DestBufTooSmallError> for StreamError {
	fn from(_: cobs::DestBufTooSmallError) -> Self {
		Self::TooMuchData
	}
}

#[cfg_attr(feature = "typescript", wasm_bindgen)]
impl Decoder {
	#[cfg(feature = "typescript")]
	pub unsafe fn new_with_global_buffer() -> Self {
		static mut BUFFER: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
		Self::new(unsafe { &mut BUFFER[..] })
	}

	pub fn feed(&mut self, buffer: &[u8]) -> Result<Option<DecodeResult>, StreamError> {
		self.decoded_size = None;

		let Some(report) = self.decoder.push(buffer)? else {
			return Ok(None);
		};

		let leftover = buffer.len() - report.parsed_size();
		self.decoded_size = Some(report.frame_size());
		Ok(Some(DecodeResult {
			leftover,
			decoded_size: report.frame_size(),
		}))
	}

	pub fn decode_request(&self) -> Result<crate::Request, StreamError> {
		let Some(decoded_size) = self.decoded_size else {
			return Err(StreamError::Incomplete);
		};

		Ok(minicbor::decode(&self.decoder.dest()[..decoded_size])?)
	}

	pub fn decode_response(&self) -> Result<crate::Response, StreamError> {
		let Some(decoded_size) = self.decoded_size else {
			return Err(StreamError::Incomplete);
		};

		Ok(minicbor::decode(&self.decoder.dest()[..decoded_size])?)
	}
}

impl Decoder {
	pub fn new(dest: &'static mut [u8]) -> Self {
		Self {
			decoder:      cobs::CobsDecoder::new(dest),
			decoded_size: None,
		}
	}
}

#[cfg_attr(feature = "typescript", wasm_bindgen)]
pub struct OffsetLength {
	pub offset: usize,
	pub len:    usize,
}

impl OffsetLength {
	pub fn get_for_slice<'a>(&self, buf: &'a [u8]) -> &'a [u8] {
		&buf[self.offset..(self.offset + self.len)]
	}
}

fn encode_raw<T: minicbor::Encode<()> + Sized>(
	request: &T,
	dest: &mut [u8],
) -> Result<OffsetLength, StreamError> {
	if dest.len() < BUFFER_SIZE {
		return Err(StreamError::TooMuchData);
	}

	let mut cursor = minicbor::encode::write::Cursor::new(&mut dest[..]);
	minicbor::encode(request, &mut cursor)?;
	let position = cursor.position();
	let frame_pos = BUFFER_SIZE >> 1;
	if position >= frame_pos {
		return Err(StreamError::TooMuchData);
	}

	// SAFETY: This is safe, we just... have to do some naughty things to get there.
	let (in_slice, out_slice) = unsafe {
		let in_slice_len = position;
		let in_slice_ptr = dest.as_ptr();
		let in_slice = core::slice::from_raw_parts(in_slice_ptr, in_slice_len);

		let out_slice_len = dest.len() - frame_pos;
		let out_slice_ptr = dest[frame_pos..].as_mut_ptr();
		let out_slice = core::slice::from_raw_parts_mut(out_slice_ptr, out_slice_len);
		(in_slice, out_slice)
	};

	let count = cobs::try_encode_including_sentinels(in_slice, out_slice)?;

	Ok(OffsetLength {
		offset: frame_pos,
		len:    count,
	})
}

#[cfg_attr(feature = "typescript", wasm_bindgen)]
pub fn encode_request(
	request: &crate::Request,
	dest: &mut [u8],
) -> Result<OffsetLength, StreamError> {
	encode_raw(request, dest)
}

#[cfg_attr(feature = "typescript", wasm_bindgen)]
pub fn encode_response(
	response: &crate::Response,
	dest: &mut [u8],
) -> Result<OffsetLength, StreamError> {
	encode_raw(response, dest)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn basic_encode_request() {
		let mut buf = [0u8; BUFFER_SIZE];
		encode_request(&crate::Request::FactoryReset, &mut buf).unwrap();
		encode_request(
			&crate::Request::StartLightProgram {
				debug:      [true, false, true],
				controller: [0xAABBCCDD; 9],
			},
			&mut buf,
		)
		.unwrap();
	}

	#[test]
	fn basic_encode_response() {
		let mut buf = [0u8; BUFFER_SIZE];
		encode_response(&crate::Response::Ok, &mut buf).unwrap();
		encode_response(&crate::Response::Uint(1337), &mut buf).unwrap();
		encode_response(
			&crate::Response::LightState {
				debug_leds:          [2398, 22, 8111],
				debug_leds_max_duty: 10000,
				controller:          [0xAABBCCDD; 9],
			},
			&mut buf,
		)
		.unwrap();
	}

	#[test]
	fn basic_encode_decode_request() {
		let mut enc_buf = [0u8; BUFFER_SIZE];
		static mut DEC_BUF: [u8; BUFFER_SIZE] = [0u8; BUFFER_SIZE];

		macro_rules! test_encdec {
			($expr:expr) => {{
				let src = $expr;
				let offlen = encode_request(&src, &mut enc_buf).unwrap();
				let mut slice = offlen.get_for_slice(&enc_buf);
				assert!(slice.len() > 0);

				let mut decoder = Decoder::new(unsafe { &mut DEC_BUF[..] });
				while slice.len() > 1 {
					let r = decoder.feed(&slice[..1]).unwrap();
					assert!(r.is_none());
					slice = &slice[1..];
				}

				let r = decoder.feed(slice).unwrap().unwrap();
				assert_eq!(r.leftover, 0);
				assert_ne!(r.decoded_size, 0);

				let req = decoder.decode_request().unwrap();
				assert_eq!(req, src);
			}};
		}

		test_encdec!(crate::Request::FactoryReset);
		test_encdec!(crate::Request::StartLightProgram {
			debug:      [true, false, true],
			controller: [0xAABBCCDD; 9],
		});
	}

	#[test]
	fn basic_encode_decode_response() {
		let mut enc_buf = [0u8; BUFFER_SIZE];
		static mut DEC_BUF: [u8; BUFFER_SIZE] = [0u8; BUFFER_SIZE];

		macro_rules! test_encdec {
			($expr:expr) => {{
				let src = $expr;
				let offlen = encode_response(&src, &mut enc_buf).unwrap();
				let mut slice = offlen.get_for_slice(&enc_buf);
				assert!(slice.len() > 0);

				let mut decoder = Decoder::new(unsafe { &mut DEC_BUF[..] });
				while slice.len() > 1 {
					let r = decoder.feed(&slice[..1]).unwrap();
					assert!(r.is_none());
					slice = &slice[1..];
				}

				let r = decoder.feed(slice).unwrap().unwrap();
				assert_eq!(r.leftover, 0);
				assert_ne!(r.decoded_size, 0);

				let req = decoder.decode_response().unwrap();
				assert_eq!(req, src);
			}};
		}

		test_encdec!(crate::Response::Uint(1337));
		test_encdec!(crate::Response::LightState {
			debug_leds:          [2398, 22, 8111],
			debug_leds_max_duty: 10000,
			controller:          [0xAABBCCDD; 9],
		});
	}
}
