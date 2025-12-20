use crc32fast::Hasher;

pub trait Crc32Ext: Sized {
	fn crc32_into(&self, hasher: &mut Hasher) {
		let bytes = unsafe {
			core::slice::from_raw_parts(
				(self as *const Self) as *const u8,
				core::mem::size_of::<Self>(),
			)
		};
		hasher.update(bytes);
	}

	fn crc32(&self) -> u32 {
		let mut hasher = Hasher::new();
		self.crc32_into(&mut hasher);
		hasher.finalize()
	}
}

impl<T: Sized> Crc32Ext for T {}
