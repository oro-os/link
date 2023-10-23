macro_rules! error {
	($($e:expr),*) => {
		#[cfg(feature = "log")]
		::log::error!($($e),*);
		#[cfg(feature = "defmt")]
		::defmt::error!($($e),*);
	}
}

pub(crate) use error;

macro_rules! debug {
	($($e:expr),*) => {
		#[cfg(feature = "log")]
		::log::debug!($($e),*);
		#[cfg(feature = "defmt")]
		::defmt::debug!($($e),*);
	}
}

pub(crate) use debug;

macro_rules! trace {
	($($e:expr),*) => {
		#[cfg(feature = "log")]
		::log::trace!($($e),*);
		#[cfg(feature = "defmt")]
		::defmt::trace!($($e),*);
	}
}

pub(crate) use trace;
