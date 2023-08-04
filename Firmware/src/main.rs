#![no_std]
#![no_main]
#![feature(const_option)]

mod arch;
mod util;

use crate::arch::SystemUnderTest;

use self::arch::{color, Arch, Color, DebugLed, IndicatorLights};
use core::fmt::Write;
#[cfg(not(test))]
use core::panic::PanicInfo;

/// The first three bytes of the MAC address are the
/// organizationally unique identifier (OUD), in this case
/// '.oO' (as ASCII -> hex).
const MAC_VENDOR: u32 = 0x2E6F4F;
sa::static_assert!(
	(MAC_VENDOR & 0x010000) == 0,
	"MAC_VENDOR cannot be a group MAC address (0:0 must be 0)"
);
sa::static_assert!(
	(MAC_VENDOR & 0x020000) != 0,
	"MAC_VENDOR must be a local MAC address (0:1 must be 1)"
);
/// The last three bytes of the system ethernet MAC address
/// are the unique device number. This is the same for all
/// link cards - 'SUT' (as ASCII -> hex).
const SYS_MAC_DEVICE: u32 = 0x535554;
/// The three octets of the external ethernet card MAC
/// address are pulled in from the environment.
const EXT_MAC_DEVICE: u32 = util::mac_str_to_int(env!("ORO_EXT_MAC_ID").as_bytes())
	.expect("ORO_EXT_MAC_ID does not match format \"AB:CD:EF\"");
/// The full system ethernet mac address
const SYS_MAC_ADDR: [u8; 6] = util::mac_bytes(MAC_VENDOR, SYS_MAC_DEVICE);
/// The full external ethernet mac address
const EXT_MAC_ADDR: [u8; 6] = util::mac_bytes(MAC_VENDOR, EXT_MAC_DEVICE);

static mut DEBUG_WRITE: Option<<self::arch::Impl as Arch>::DebugSerialImpl> = None;

#[doc(hidden)]
pub fn _debug_print(args: ::core::fmt::Arguments) {
	if let Some(write) = unsafe { &mut DEBUG_WRITE } {
		write.write_fmt(args).unwrap();
	}
}

#[macro_export]
macro_rules! print {
	($($arg:tt)*) => ($crate::_debug_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
	() => ($crate::print!("\n"));
	($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[cfg(not(test))]
#[panic_handler]
fn panic(panic: &PanicInfo<'_>) -> ! {
	println!("PANIC: {:#?}", panic);
	loop {}
}

macro_rules! sleep_ticks {
	($n:literal) => {
		for _ in 0..$n {
			unsafe {
				::core::arch::asm!("NOP");
			}
		}
	};
}

#[no_mangle]
pub fn main() -> ! {
	let (mut dbgled, dbgserial, mut indlights, mut power_controller, mut interfaces) = unsafe {
		arch::Impl::initialize(arch::ArchConfig {
			ext_eth_mac: EXT_MAC_ADDR,
			sys_eth_mac: SYS_MAC_ADDR,
		})
	};
	unsafe {
		DEBUG_WRITE = Some(dbgserial);
	}

	println!(
		"Oro Link x86 rev6 firmware (version {})",
		env!("CARGO_PKG_VERSION")
	);
	println!("beginning POST:");

	dbgled.on();
	sleep_ticks!(500000);
	dbgled.off();
	println!("... debug led OK");

	indlights.enable();

	const COLORS: [Color; 8] = [
		color::BLACK,
		color::WHITE,
		color::RED,
		color::YELLOW,
		color::GREEN,
		color::CYAN,
		color::BLUE,
		color::MAGENTA,
	];

	for color_idx in 0..COLORS.len() {
		indlights.first(COLORS[color_idx % COLORS.len()]);
		indlights.second(COLORS[(color_idx + 1) % COLORS.len()]);
		indlights.third(COLORS[(color_idx + 2) % COLORS.len()]);
		sleep_ticks!(500000);
	}

	indlights.all_off();

	println!("... indicator lights OK");

	power_controller.set_power_state(arch::PowerState::Standby);
	println!("... psu standby OK");
	sleep_ticks!(10000000);
	power_controller.set_power_state(arch::PowerState::On);
	println!("... psu power OK");
	sleep_ticks!(10000000);
	power_controller.set_power_state(arch::PowerState::Off);
	println!("... psu OK");
	sleep_ticks!(10000000);

	println!("... ORO LINK POST OK");

	// XXX DEBUG
	{
		use smoltcp::iface::{SocketSet, SocketStorage};
		use smoltcp::time::Instant;
		use smoltcp::wire::{IpAddress, IpCidr, Ipv4Address};

		let mut exteth = interfaces.external;
		exteth.iface.update_ip_addrs(|ip_addrs| {
			ip_addrs
				.push(IpCidr::new(IpAddress::v4(10, 0, 0, 142), 24))
				.unwrap();
		});
		exteth
			.iface
			.routes_mut()
			.add_default_ipv4_route(Ipv4Address::new(10, 0, 0, 42))
			.unwrap();

		let mut socket_storage = [
			SocketStorage::default(),
			SocketStorage::default(),
			SocketStorage::default(),
		];
		let mut sockets = SocketSet::new(&mut socket_storage[..]);

		let mut iters = 0;
		loop {
			iters += 1;
			let timestamp = Instant::from_millis(iters);

			exteth
				.iface
				.poll(timestamp, &mut exteth.device, &mut sockets);

			sleep_ticks!(5000);
		}
	}
}
