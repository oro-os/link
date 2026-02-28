#![cfg_attr(not(feature = "std"), no_std)]
#![feature(const_cmp, const_trait_impl)]

pub use minicbor;
#[cfg(feature = "heapless")]
mod heapless_cbor;
pub mod stream;

#[derive(minicbor::Encode, minicbor::Decode)]
#[cfg_attr(
	any(feature = "serde", target_arch = "wasm32"),
	derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi, from_wasm_abi))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Eq)]
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
	/// Uint
	#[n(6)]
	GetFrameCount,
	/// BulkTransfer
	#[n(7)]
	GetFrame,
	/// LightState
	#[n(8)]
	GetLightState,
	/// Ok
	/// Err(InitOnly)
	#[n(9)]
	StartLightProgram {
		#[n(0)]
		debug:      [bool; 3],
		#[n(1)]
		controller: [u32; 9],
	},
	/// Ok
	/// Err(InitOnly)
	#[n(10)]
	EndLightProgram,
}

#[derive(minicbor::Encode, minicbor::Decode)]
#[cfg_attr(
	any(feature = "serde", target_arch = "wasm32"),
	derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi, from_wasm_abi))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Eq)]
pub enum Response {
	#[n(0)]
	Ok,
	#[n(1)]
	Uint(#[n(0)] u64),
	#[n(2)]
	Err(#[n(0)] Error),
	#[n(3)]
	BulkTransfer(#[n(0)] u64),
	#[n(4)]
	LightState {
		#[n(0)]
		debug_leds:          [u16; 3],
		#[n(1)]
		debug_leds_max_duty: u16,
		/// 36 u8's packed BE into 9 u32s
		#[n(2)]
		controller:          [u32; 9],
	},
	#[cfg(all(feature = "std", not(feature = "defmt")))]
	#[n(5)]
	String(#[n(0)] String),
	#[cfg(feature = "heapless")]
	#[n(5)]
	String(
		#[cbor(with = "heapless_cbor")]
		#[n(0)]
		heapless::String<64>,
	),
}

#[derive(minicbor::Encode, minicbor::Decode)]
#[cfg_attr(
	any(feature = "serde", target_arch = "wasm32"),
	derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[cfg_attr(feature = "typescript", tsify(into_wasm_abi, from_wasm_abi))]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
	#[n(0)]
	TooLong,
	#[n(1)]
	MalformedRequest,
	#[n(2)]
	InitOnly,
}
