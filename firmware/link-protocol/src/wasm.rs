use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn encode_request(value: crate::Request) -> Result<Vec<u8>, JsError> {
	let mut v = vec![0, 0, 0, 0];
	minicbor::encode(value, &mut v)?;
	let size = (v.len() - 4) as u32;
	let size_bytes = u32::to_be_bytes(size);
	v[0..4].copy_from_slice(&size_bytes[..]);
	Ok(v)
}

#[wasm_bindgen]
pub fn decode_response(value: &[u8]) -> Result<crate::Response, JsError> {
	Ok(minicbor::decode(&value)?)
}
