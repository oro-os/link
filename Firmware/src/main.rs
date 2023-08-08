#![no_std]
#![no_main]
#![feature(type_alias_impl_trait, core_intrinsics)]

mod chip;
mod uc;

#[cfg(not(test))]
use core::panic::PanicInfo;
use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::{Duration, Instant, Timer};
use static_cell::make_static;
use uc::{DebugLed, Monitor as _};

#[cfg(not(test))]
#[panic_handler]
fn panic(panic: &PanicInfo<'_>) -> ! {
	error!(
		"PANIC @ {}:{}: {}",
		panic.location().map(|l| l.file()).unwrap_or("?"),
		panic.location().map(|l| l.line()).unwrap_or(0),
		panic
			.payload()
			.downcast_ref::<&str>()
			.unwrap_or(&"<unknown>")
	);
	loop {}
}

type ExtEthDriver = impl uc::EthernetDriver;
static mut EXT_ETH_STACK: Option<Stack<ExtEthDriver>> = None;

#[embassy_executor::task]
async fn net_task() {
	unsafe { EXT_ETH_STACK.as_ref().unwrap().run().await };
}

type Monitor = impl uc::Monitor;
static mut MONITOR: Option<Monitor> = None;

#[embassy_executor::task]
async fn monitor_task() {
	let mut monitor = unsafe { MONITOR.take().unwrap() };
	loop {
		let millis = Instant::now().as_millis();
		monitor.tick(millis);
		Timer::after(Duration::from_millis(1000 / 240)).await;
	}
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) {
	let (mut debug_led, _system, monitor, exteth) = uc::init(&spawner).await;

	info!(
		"Oro Link x86 booting (version {})",
		env!("CARGO_PKG_VERSION")
	);

	// Let peripherals power on
	Timer::after(Duration::from_millis(300)).await;

	unsafe {
		MONITOR = {
			fn init(monitor: Monitor) -> Option<Monitor> {
				Some(monitor)
			}
			init(monitor)
		};
	}

	let extnet = {
		let seed = [0; 8]; // TODO use RNG from `uc` module
		let seed = u64::from_le_bytes(seed);

		let config = embassy_net::Config::dhcpv4(Default::default());

		Stack::new(
			exteth,
			config,
			make_static!(embassy_net::StackResources::<2>::new()),
			seed,
		)
	};

	let _extnet = unsafe {
		EXT_ETH_STACK = {
			fn init(extnet: Stack<ExtEthDriver>) -> Option<Stack<ExtEthDriver>> {
				Some(extnet)
			}
			init(extnet)
		};

		EXT_ETH_STACK.as_ref().unwrap()
	};

	spawner.spawn(net_task()).unwrap();
	spawner.spawn(monitor_task()).unwrap();

	loop {
		debug_led.on();
		Timer::after(Duration::from_millis(100)).await;
		debug_led.off();
		Timer::after(Duration::from_millis(3000)).await;
	}
}
