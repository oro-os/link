#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

mod chip;
mod uc;

#[cfg(not(test))]
use core::panic::PanicInfo;
use defmt::{error, info, warn};
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use static_cell::make_static;
use uc::DebugLed;

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

#[embassy_executor::main]
pub async fn main(spawner: Spawner) {
	let (mut debug_led, _system, _indicators, exteth) = uc::init();

	info!(
		"Oro Link x86 booting (version {})",
		env!("CARGO_PKG_VERSION")
	);

	// Let peripherals power on
	Timer::after(Duration::from_millis(300)).await;

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

	let extnet = unsafe {
		EXT_ETH_STACK = {
			fn init(extnet: Stack<ExtEthDriver>) -> Option<Stack<ExtEthDriver>> {
				Some(extnet)
			}
			init(extnet)
		};

		EXT_ETH_STACK.as_ref().unwrap()
	};

	spawner.spawn(net_task()).unwrap();

	loop {
		debug_led.on();
		Timer::after(Duration::from_millis(100)).await;
		debug_led.off();
		Timer::after(Duration::from_millis(3000)).await;

		match extnet
			.dns_query("oro.sh", embassy_net::dns::DnsQueryType::A)
			.await
		{
			Ok(addr) => {
				info!("resolved: oro.sh @ {:?}", addr[0]);
			}
			Err(err) => {
				warn!("resolved: oro.sh FAILED: {:?}", err);
			}
		}
	}
}
