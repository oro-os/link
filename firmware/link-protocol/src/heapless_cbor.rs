use minicbor::{
	decode::{Decoder, Error as DecError},
	encode::{Encoder, Error as EncError, Write},
};

pub fn encode<Ctx, W: Write, const SZ: usize>(
	v: &heapless::String<SZ>,
	e: &mut Encoder<W>,
	_ctx: &mut Ctx,
) -> Result<(), EncError<W::Error>> {
	e.encode(v.as_str())?;
	Ok(())
}

pub fn decode<'b, Ctx, const SZ: usize>(
	d: &mut Decoder<'b>,
	_ctx: &mut Ctx,
) -> Result<heapless::String<SZ>, DecError> {
	let slice = d.str()?;
	let mut s = heapless::String::new();
	s.insert_str(0, slice)
		.map_err(|_| DecError::message("string contents too long for heapless string"))?;
	Ok(s)
}
