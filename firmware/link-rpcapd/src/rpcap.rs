//! Taken from:
//! https://github.com/the-tcpdump-group/libpcap/blob/master/rpcap-protocol.h
//!
//! NOTE: This protocol only supports as much as is needed in order to handle
//!       the usecases of this daemon and thus may not be complete. **DO NOT USE
//!       THIS CODE AS AN AUTHORITATIVE SOURCE OF THIS PROTOCOL DESIGN.**
//!
//! NOTE: Friendly reminder that ALL messages must be padded to 32 bits!

use async_std::io::{self, ReadExt, WriteExt};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub enum AuthType {
	Null,
	Basic,
	Unknown,
}

/// We handle the flags internally;
/// always sent are the following, regardless of
/// actual link status.
///
/// - PCAP_IF_UP (0x00000002)
/// - PCAP_IF_RUNNING (0x00000004)
/// - PCAP_IF_CONNECTION_STATUS_CONNECTED (0x00000010)
#[derive(Clone, Debug)]
pub struct Interface {
	pub name: String,
	pub description: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum RPCAPMessage {
	AuthRequest {
		auth_type: AuthType,
	},
	/// Indicates Auth OK
	AuthResponse,
	Error(u16, String),
	AuthNotSupError(String),
	WrongMessageError(String),
	OpenError(String),
	OpenDeviceRequest {
		device_name: String,
	},
	/// Device is always base10 ethernet.
	OpenDeviceResponse,
	FindAllDevsRequest,
	FindAllDevsResponse(Vec<Interface>),
	StartCapRequest,
	StartCapResponse {
		server_port: u16,
	},
	/// We don't support these. We just acknowledge them.
	UpdateFilterRequest,
	UpdateFilterResponse,
	Packet {
		data: Vec<u8>,
		arrival_time: SystemTime,
		number: usize,
	},
}

impl RPCAPMessage {
	pub async fn encode<R: WriteExt + Unpin + Sized>(&self, s: &mut R) -> Result<(), io::Error> {
		s.encode(&0u8).await?; // version

		match self {
			RPCAPMessage::AuthNotSupError(message) => {
				s.encode(&0x01u8).await?;
				s.encode_be(&20u16).await?;
				let bytes = message.as_bytes();
				s.encode_be(&(bytes.len() as u32)).await?;
				s.write_all(bytes).await?;
			}
			RPCAPMessage::WrongMessageError(message) => {
				s.encode(&0x01u8).await?;
				s.encode_be(&16u16).await?;
				let bytes = message.as_bytes();
				s.encode_be(&(bytes.len() as u32)).await?;
				s.write_all(bytes).await?;
			}
			RPCAPMessage::OpenError(message) => {
				s.encode(&0x01u8).await?;
				s.encode_be(&6u16).await?;
				let bytes = message.as_bytes();
				s.encode_be(&(bytes.len() as u32)).await?;
				s.write_all(bytes).await?;
			}
			RPCAPMessage::AuthResponse => {
				s.encode(&(0x08u8 | 0x80)).await?;
				s.encode_be(&0u16).await?;
				s.encode_be(&0u32).await?;
			}
			RPCAPMessage::OpenDeviceResponse => {
				s.encode(&(0x03u8 | 0x80)).await?;
				s.encode_be(&0u16).await?;
				s.encode_be(&8u32).await?;
				s.encode_be(&1u32).await?; // link type 1 = base10 eth, should be i16
				s.encode_be(&0u32).await?; // timezone offset - not used.
			}
			RPCAPMessage::FindAllDevsResponse(devs) => {
				s.encode(&(0x02u8 | 0x80)).await?;
				debug_assert!(devs.len() < u16::MAX as usize);
				s.encode_be(&(devs.len() as u16)).await?;

				// Calculate length
				let mut payload_length = 0;
				for dev in devs {
					payload_length += 12; // base structure length
					payload_length += dev.name.as_bytes().len();
					payload_length += dev.description.as_bytes().len();
				}

				s.encode_be(&(payload_length as u32)).await?;

				for dev in devs {
					let name_bytes = dev.name.as_bytes();
					let description_bytes = dev.description.as_bytes();

					debug_assert!(name_bytes.len() <= u16::MAX as usize);
					debug_assert!(description_bytes.len() <= u16::MAX as usize);

					s.encode_be(&(name_bytes.len() as u16)).await?;
					s.encode_be(&(description_bytes.len() as u16)).await?;
					s.encode_be(
						&(
							// PCAP_IF_UP
							0x00000002u32
							// PCAP_IF_RUNNING
							| 0x00000004
							// PCAP_IF_CONNECTION_STATUS_CONNECTED
							| 0x00000010
						),
					)
					.await?;
					s.encode_be(&0u16).await?; // # addrs (none)
					s.encode_be(&0u16).await?; // dummy

					s.write_all(name_bytes).await?;
					s.write_all(description_bytes).await?;
				}
			}
			RPCAPMessage::StartCapResponse { server_port } => {
				s.encode(&(0x04u8 | 0x80)).await?;
				s.encode_be(&0u16).await?;
				s.encode_be(&8u32).await?;
				s.encode_be(&1514u32).await?; // buf size we allocated (we can lie here, I think - it's also supposed to be signed)
				s.encode_be(server_port).await?; // port of the server (in passive mode, which we are)
				s.encode_be(&0u16).await?; // dummy
			}
			RPCAPMessage::UpdateFilterResponse => {
				s.encode(&(0x05u8 | 0x80)).await?;
				s.encode_be(&0u16).await?;
				s.encode_be(&0u32).await?;
			}
			RPCAPMessage::Packet {
				data,
				arrival_time,
				number,
			} => {
				s.encode(&0x07u8).await?;
				s.encode_be(&0u16).await?;
				s.encode_be(&(data.len() as u32 + 20)).await?;
				let time = arrival_time.duration_since(UNIX_EPOCH).unwrap();
				s.encode_be(&(time.as_secs() as u32)).await?;
				s.encode_be(&((time.as_micros() % 1000000) as u32)).await?;
				s.encode_be(&(data.len() as u32)).await?;
				s.encode_be(&(data.len() as u32)).await?;
				s.encode_be(&(*number as u32)).await?;
				s.write_all(data).await?;
			}
			msg => {
				panic!("tried to send an unsupported outbound message type: {msg:?}");
			}
		}

		Ok(())
	}

	pub async fn parse<R: ReadExt + Unpin + Sized>(s: &mut R) -> Result<Self, io::Error> {
		// Read RPCAP header
		let _version: u8 = s.parse().await?;
		let message_type: u8 = s.parse().await?;
		let message_value: u16 = s.parse_be().await?; // XXX may not be BE here.
		let payload_length: u32 = s.parse_be().await?;

		match message_type {
			// Error
			0x01 => {
				let mut error_buf = vec![0u8; payload_length as usize];
				s.read_exact(&mut error_buf[..]).await?;
				let error = String::from_utf8_lossy(&error_buf[..]).to_string();
				Ok(RPCAPMessage::Error(message_value, error))
			}

			// Auth request
			0x08 => {
				let auth_type: u16 = s.parse_be().await?;
				let _: u16 = s.parse_be().await?;
				let _auth_field_1: u16 = s.parse_be().await?;
				let _auth_field_2: u16 = s.parse_be().await?;

				Ok(RPCAPMessage::AuthRequest {
					auth_type: match auth_type {
						0 => AuthType::Null,
						1 => AuthType::Basic,
						_ => AuthType::Unknown,
					},
				})
			}

			// Open device request
			0x03 => {
				// FIXME: I suppose this is a security bug. The payload length can be
				// 32-bits worth of bytes to allocate. This would very much cause problems.
				// There should probably be a cap here. Just, uh... don't use this thing on
				// the WAN, please.
				let mut device_name_buf = vec![0u8; payload_length as usize];
				s.read_exact(&mut device_name_buf[..]).await?;
				let device_name = String::from_utf8_lossy(&device_name_buf[..]).to_string();
				Ok(RPCAPMessage::OpenDeviceRequest { device_name })
			}

			// Find all IF devs request
			0x02 => Ok(RPCAPMessage::FindAllDevsRequest),

			// Start capture request
			0x04 => {
				// We don't actually use any of the structure fields they send us.
				if payload_length > 0 {
					let mut dummy_buf = vec![0u8; payload_length as usize];
					s.read_exact(&mut dummy_buf[..]).await?;
				}
				Ok(RPCAPMessage::StartCapRequest)
			}

			// Update filter request (we ignore these)
			0x05 => {
				// Again, we skip all of this.
				if payload_length > 0 {
					let mut dummy_buf = vec![0u8; payload_length as usize];
					s.read_exact(&mut dummy_buf[..]).await?;
				}
				Ok(RPCAPMessage::UpdateFilterRequest)
			}

			// Unknown
			_ => Err(io::Error::new(
				io::ErrorKind::Unsupported,
				format!("unsupported message code from client: {message_type:X}"),
			)),
		}
	}
}

trait TranscoderType
where
	Self: Sized,
{
	async fn parse<R: ReadExt + Unpin>(s: &mut R) -> Result<Self, io::Error>;
	async fn encode<W: WriteExt + Unpin>(&self, s: &mut W) -> Result<(), io::Error>;
}

trait MultibyteTranscoderType
where
	Self: Sized,
{
	async fn parse_be<R: ReadExt + Unpin>(s: &mut R) -> Result<Self, io::Error>;
	async fn encode_be<W: WriteExt + Unpin>(&self, s: &mut W) -> Result<(), io::Error>;
}

trait TypedParser: ReadExt + Unpin + Sized {
	#[inline]
	async fn parse<T: TranscoderType>(&mut self) -> Result<T, io::Error> {
		T::parse(self).await
	}

	#[inline]
	async fn parse_be<T: MultibyteTranscoderType>(&mut self) -> Result<T, io::Error> {
		T::parse_be(self).await
	}
}

impl<T: ReadExt + Unpin + Sized> TypedParser for T {}

trait TypedEncoder: WriteExt + Unpin + Sized {
	#[inline]
	async fn encode<T: TranscoderType>(&mut self, v: &T) -> Result<(), io::Error> {
		v.encode(self).await
	}

	#[inline]
	async fn encode_be<T: MultibyteTranscoderType>(&mut self, v: &T) -> Result<(), io::Error> {
		v.encode_be(self).await
	}
}

impl<T: WriteExt + Unpin + Sized> TypedEncoder for T {}

impl TranscoderType for u8 {
	async fn parse<R: ReadExt + Unpin>(s: &mut R) -> Result<Self, io::Error> {
		let mut buf = [0u8];
		s.read_exact(&mut buf[..]).await?;
		Ok(buf[0])
	}

	async fn encode<W: WriteExt + Unpin>(&self, s: &mut W) -> Result<(), io::Error> {
		s.write_all(&[*self]).await
	}
}

impl MultibyteTranscoderType for u16 {
	async fn parse_be<R: ReadExt + Unpin>(s: &mut R) -> Result<Self, io::Error> {
		let mut buf = [0u8; 2];
		s.read_exact(&mut buf[..]).await?;
		Ok(u16::from_be_bytes(buf))
	}

	async fn encode_be<W: WriteExt + Unpin>(&self, s: &mut W) -> Result<(), io::Error> {
		let buf = (*self).to_be_bytes();
		s.write_all(&buf).await
	}
}

impl MultibyteTranscoderType for u32 {
	async fn parse_be<R: ReadExt + Unpin>(s: &mut R) -> Result<Self, io::Error> {
		let mut buf = [0u8; 4];
		s.read_exact(&mut buf[..]).await?;
		Ok(u32::from_be_bytes(buf))
	}

	async fn encode_be<W: WriteExt + Unpin>(&self, s: &mut W) -> Result<(), io::Error> {
		let buf = (*self).to_be_bytes();
		s.write_all(&buf).await
	}
}
