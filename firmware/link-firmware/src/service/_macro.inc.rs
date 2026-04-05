use crate::channel::ChannelExt;
use static_cell::StaticCell;

macro_rules! services {
	{
		$(
			$(#[config($config:tt)])?
			$(#[bus($bus:tt)])?
			$(#[rx($rx:tt)])?
			$(#[skip($skip:tt)])?
			$name:ident
		),+
		$(,)?
	} => {
		/// Main bus for service channels.
		///
		/// Passed to every service.
		#[derive(Clone)]
		#[allow(unused)]
		pub struct Bus {
			$(
				$(#[cfg($rx)])?
				pub $name: <self::$name::Channel as ChannelExt>::Sender,
			)+
		}

		pub struct BusConfig {
			$(
				$(#[cfg($config)])?
				pub $name: self::$name::Config,
			)+
		}

		impl BusConfig {
			pub fn spawn_all(self, spawner: embassy_executor::Spawner) {
				#[allow(unused)]
				struct OwnedBus {
					$(
						$(#[cfg($rx)])?
						$name: self::$name::Channel,
					)+
				}

				static CHANNELS: StaticCell<OwnedBus> = StaticCell::new();
				let owned_bus = CHANNELS.init(OwnedBus {$(
					$(#[cfg($rx)])?
					$name: self::$name::Channel::new(),
				)+});

				let bus = Bus {
					$(
						$(#[cfg($rx)])?
						$name: owned_bus.$name.sender()
					),+
				};

				$(
					if cfg!(any(false, $($skip)?)) {
						defmt::info!(
							"skipping service (due to #[skip]): {}",
							::core::stringify!($name)
						);
					} else {
						defmt::info!("spawning service: {}", ::core::stringify!($name));
						spawner.spawn(services!(
							@CALL
							self::$name::run,
							()
							{$($bus)?} => {bus.clone()}
							{$($rx)?} => {&owned_bus.$name}
							{$($config)?} => {self.$name}
						).unwrap());
					}
				)+
			}
		}
	};

	(@CALL $f:expr, ($($args:tt)*)) => { $f($($args)*) };
	(@CALL $f:expr, () {false} => {$($tt:tt)*} $($params:tt)*) => {
        services!(@CALL $f, () $($params)*)
    };
    (@CALL $f:expr, () {$(true)?} => {$($tt:tt)*} $($params:tt)*) => {
        services!(@CALL $f, ($($tt)*) $($params)*)
    };
    (@CALL $f:expr, ($($args:tt)*) {false} => {$($tt:tt)*} $($params:tt)*) => {
        services!(@CALL $f, ($($args)*) $($params)*)
    };
    (@CALL $f:expr, ($($args:tt)*) {$(true)?} => {$($tt:tt)*} $($params:tt)*) => {
        services!(@CALL $f, ($($args)*, $($tt)*) $($params)*)
    };
}

#[macro_export]
macro_rules! service_config {
	($($name:ident { $($tt:tt)* }),+ $(,)?) => {
		$crate::service::BusConfig {
			$(
				$name: $crate::service::$name::Config { $($tt)* },
			)+
		}
	}
}
