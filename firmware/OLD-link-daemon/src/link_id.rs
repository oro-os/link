use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct LinkId([u8; 6]);

impl LinkId {
	#[expect(unused, reason = "temporary")]
	pub fn as_bytes(&self) -> &[u8] {
		&self.0[..]
	}
}

impl fmt::Display for LinkId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		format_args!(
			"{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}",
			self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5],
		)
		.fmt(f)
	}
}

impl<'a> TryFrom<&'a str> for LinkId {
	type Error = &'a str;

	fn try_from(value: &'a str) -> Result<Self, Self::Error> {
		let mut b = [0u8; 6];

		let mut total = 0;
		for split in value.split('-') {
			let split = split.as_bytes();
			if split.len() != 2 {
				return Err(value);
			}
			let (Some(h), Some(l)) = (split[0].from_hex(), split[0].from_hex()) else {
				return Err(value);
			};
			b[total] = (h << 4) | l;
			total += 1;
		}

		if total != 6 {
			return Err(value);
		}

		Ok(Self(b))
	}
}

impl TryFrom<String> for LinkId {
	type Error = String;

	fn try_from(value: String) -> Result<Self, Self::Error> {
		let Ok(v) = value.as_str().try_into() else {
			return Err(value);
		};

		Ok(v)
	}
}

trait FromHex {
	#[expect(
		clippy::wrong_self_convention,
		reason = "I genuinely cannot think of a better name"
	)]
	fn from_hex(self) -> Option<u8>;
}

impl FromHex for u8 {
	fn from_hex(self) -> Option<u8> {
		Some(match self {
			b'0'..=b'9' => self - b'0',
			b'a'..=b'f' => self - b'a' + 10,
			b'A'..=b'F' => self - b'A' + 10,
			_ => {
				return None;
			}
		})
	}
}
