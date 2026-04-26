use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct LinkId([u8; 6]);

impl fmt::Display for LinkId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			"{:02X}-{:02X}-{:02X}-{:02X}-{:02X}-{:02X}",
			self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5],
		)
	}
}

impl<'a> TryFrom<&'a str> for LinkId {
	type Error = &'a str;

	fn try_from(value: &'a str) -> Result<Self, Self::Error> {
		let mut bytes = [0u8; 6];

		for (index, split) in value.split('-').enumerate() {
			if index >= bytes.len() || split.len() != 2 {
				return Err(value);
			}

			bytes[index] = u8::from_str_radix(split, 16).map_err(|_| value)?;
		}

		if value.split('-').count() != bytes.len() {
			return Err(value);
		}

		Ok(Self(bytes))
	}
}

impl TryFrom<String> for LinkId {
	type Error = String;

	fn try_from(value: String) -> Result<Self, Self::Error> {
		match value.as_str().try_into() {
			Ok(link_id) => Ok(link_id),
			Err(_) => Err(value),
		}
	}
}
