const fn asc_to_hex(b: u8) -> Option<u8> {
	// Verbosity is easiest/most maintainable
	// way to make this compile-time. Sorry!
	Some(match b {
		b'0' => 0,
		b'1' => 1,
		b'2' => 2,
		b'3' => 3,
		b'4' => 4,
		b'5' => 5,
		b'6' => 6,
		b'7' => 7,
		b'8' => 8,
		b'9' => 9,
		b'a' => 10,
		b'b' => 11,
		b'c' => 12,
		b'd' => 13,
		b'e' => 14,
		b'f' => 15,
		b'A' => 10,
		b'B' => 11,
		b'C' => 12,
		b'D' => 13,
		b'E' => 14,
		b'F' => 15,
		_ => return None,
	})
}

/// Converts a `"AB:CD:EF"` string into `Some(0xABCDEFu32)`,
/// or None if the pattern doesn't match.
pub const fn mac_str_to_int(bytes: &[u8]) -> Option<u32> {
	// Sorry about the verbosity in this function, it's the only way
	// to make it compile-time from what I can muster.
	if bytes.len() != 8 && bytes.len() != 10 {
		return None;
	}

	let long = bytes.len() == 10;
	let o = if long { 1 } else { 0 };

	if long && !(bytes[0] == b'"' || bytes[0] == b'\'') {
		return None;
	}
	let first = bytes[0];
	let r = if let Some(x) = asc_to_hex(bytes[o + 0]) {
		x as u32
	} else {
		return None;
	};
	let r = (r << 4)
		| if let Some(x) = asc_to_hex(bytes[o + 1]) {
			x as u32
		} else {
			return None;
		};
	if bytes[o + 2] != b':' {
		return None;
	}
	let r = (r << 4)
		| if let Some(x) = asc_to_hex(bytes[o + 3]) {
			x as u32
		} else {
			return None;
		};
	let r = (r << 4)
		| if let Some(x) = asc_to_hex(bytes[o + 4]) {
			x as u32
		} else {
			return None;
		};
	if bytes[o + 5] != b':' {
		return None;
	}
	let r = (r << 4)
		| if let Some(x) = asc_to_hex(bytes[o + 6]) {
			x as u32
		} else {
			return None;
		};
	let r = (r << 4)
		| if let Some(x) = asc_to_hex(bytes[o + 7]) {
			x as u32
		} else {
			return None;
		};
	if long && bytes[9] != first {
		return None;
	}

	Some(r)
}

/// Converts an OUM and device ID to a mac address octet array
pub const fn mac_bytes(oum: u32, device: u32) -> [u8; 6] {
	[
		((oum >> 16) & 0xFF) as u8,
		((oum >> 8) & 0xFF) as u8,
		(oum & 0xFF) as u8,
		((device >> 16) & 0xFF) as u8,
		((device >> 8) & 0xFF) as u8,
		(device & 0xFF) as u8,
	]
}
