#![cfg_attr(not(any(test, target_arch = "wasm32")), no_std)]

#[cfg(target_arch = "wasm32")]
mod wasm;

pub use minicbor;

#[derive(minicbor::Encode, minicbor::Decode)]
#[cfg_attr(
	any(feature = "serde", target_arch = "wasm32"),
	derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi, from_wasm_abi))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Request {
	/// Uint
	#[n(0)]
	GetVersionMajor,
	/// Uint
	#[n(1)]
	GetVersionMinor,
	/// Uint
	#[n(2)]
	GetVersionPatch,
	/// Uint
	#[n(3)]
	IsInInitMode,
	/// Ok
	/// Err(InitOnly)
	#[n(4)]
	FinishInitMode,
	/// Ok
	/// Err(InitOnly)
	#[n(5)]
	FactoryReset,
}

#[derive(minicbor::Encode, minicbor::Decode)]
#[cfg_attr(
	any(feature = "serde", target_arch = "wasm32"),
	derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi, from_wasm_abi))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Response {
	#[n(0)]
	Ok,
	#[n(1)]
	Uint(#[n(0)] u64),
	#[n(2)]
	Err(#[n(0)] Error),
}

#[derive(minicbor::Encode, minicbor::Decode)]
#[cfg_attr(
	any(feature = "serde", target_arch = "wasm32"),
	derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi, from_wasm_abi))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
	#[n(0)]
	TooLong,
	#[n(1)]
	MalformedRequest,
	#[n(2)]
	InitOnly,
}
