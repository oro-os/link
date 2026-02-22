pub trait Relaxed<T> {
	#[must_use]
	fn get(&self) -> T;
	fn set(&self, v: T) -> T;
}

pub trait NumericRelaxed<T> {
	fn increment(&self) -> T;
}

macro_rules! impl_relaxed {
	($($ty:ident => $pty:ty),* $(,)?) => {$(
		impl Relaxed<$pty> for core::sync::atomic::$ty {
			#[inline]
			fn get(&self) -> $pty {
				self.load(core::sync::atomic::Ordering::Relaxed)
			}

			#[inline]
			fn set(&self, v: $pty) -> $pty {
				self.swap(v, core::sync::atomic::Ordering::Relaxed)
			}
		}
	)*}
}

impl_relaxed! {
	AtomicU8 => u8,
	AtomicU16 => u16,
	AtomicU32 => u32,
	AtomicUsize => usize,
	AtomicI8 => i8,
	AtomicI16 => i16,
	AtomicI32 => i32,
	AtomicIsize => isize,
	AtomicBool => bool,
}

macro_rules! impl_numeric_relaxed {
	($($ty:ident => $pty:ty),* $(,)?) => {$(
		impl NumericRelaxed<$pty> for core::sync::atomic::$ty {
			fn increment(&self) -> $pty {
				self.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
			}
		}
	)*}
}

impl_numeric_relaxed! {
	AtomicU8 => u8,
	AtomicU16 => u16,
	AtomicU32 => u32,
	AtomicUsize => usize,
	AtomicI8 => i8,
	AtomicI16 => i16,
	AtomicI32 => i32,
	AtomicIsize => isize,
}
