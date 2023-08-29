#![no_std]
#![no_main]
#![feature(type_alias_impl_trait, core_intrinsics)]

mod chip;
mod font;
mod net;
mod uc;

use core::cell::RefCell;
#[cfg(not(test))]
use core::panic::PanicInfo;
use defmt::{debug, error, info};
use embassy_executor::Spawner;
use embassy_net::{ConfigV4, Ipv4Address, Stack};
use embassy_time::{Duration, Instant, Timer};
use static_cell::make_static;
use uc::{DebugLed, LogSeverity, Monitor as _, Scene};

#[defmt::panic_handler]
fn defmt_panic() -> ! {
	#[allow(clippy::empty_loop)]
	loop {}
}

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
	#[allow(clippy::empty_loop)]
	loop {}
}

type ExtEthDriver = impl uc::EthernetDriver;

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<ExtEthDriver>) {
	stack.run().await;
}

type Monitor = impl uc::Monitor;
static mut MONITOR: Option<RefCell<Monitor>> = None;

#[embassy_executor::task]
async fn monitor_task() {
	loop {
		{
			let mut monitor = unsafe { MONITOR.as_ref().unwrap().borrow_mut() };
			let millis = Instant::now().as_millis();
			monitor.tick(millis);
		}
		Timer::after(Duration::from_millis(1000 / 240)).await;
	}
}

type ImplDebugLed = impl uc::DebugLed;
static mut DEBUG_LED: Option<ImplDebugLed> = None;

#[embassy_executor::task]
async fn blink_debug_led() {
	let mut debug_led = unsafe { DEBUG_LED.take().unwrap() };
	loop {
		debug_led.on();
		Timer::after(Duration::from_millis(100)).await;
		debug_led.off();
		Timer::after(Duration::from_millis(2000)).await;
	}
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) {
	let (debug_led, _system, monitor, exteth) = uc::init(&spawner).await;

	// Let peripherals power on
	Timer::after(Duration::from_millis(50)).await;

	unsafe {
		MONITOR = {
			fn init(monitor: Monitor) -> Option<RefCell<Monitor>> {
				Some(RefCell::new(monitor))
			}
			init(monitor)
		};
	}

	info!(
		"Oro Link x86 booting (version {})",
		env!("CARGO_PKG_VERSION")
	);

	unsafe {
		MONITOR.as_ref().unwrap().borrow_mut().set_scene(Scene::Log);
	}

	LogSeverity::Info.log(
		unsafe { MONITOR.as_ref().unwrap() },
		"booting oro link...".into(),
	);

	let extnet = {
		let seed = [0; 8]; // TODO use RNG from `uc` module
		let seed = u64::from_le_bytes(seed);

		let config = embassy_net::Config::dhcpv4(Default::default());

		&*make_static!(Stack::new(
			exteth,
			config,
			make_static!(embassy_net::StackResources::<16>::new()),
			seed,
		))
	};

	unsafe {
		DEBUG_LED = {
			fn init(debugled: ImplDebugLed) -> Option<ImplDebugLed> {
				Some(debugled)
			}

			init(debug_led)
		};
	}

	spawner.spawn(net_task(extnet)).unwrap();
	spawner.spawn(monitor_task()).unwrap();
	spawner.spawn(blink_debug_led()).unwrap();

	LogSeverity::Info.log(
		unsafe { MONITOR.as_ref().unwrap() },
		"waiting for DHCP lease...".into(),
	);

	loop {
		if extnet.is_config_up() {
			break;
		}

		Timer::after(Duration::from_millis(100)).await;
		debug!("still waiting for config up...");
	}

	debug!("config is up");

	LogSeverity::Info.log(
		unsafe { MONITOR.as_ref().unwrap() },
		"reconfiguring DNS...".into(),
	);

	Timer::after(Duration::from_millis(100)).await;

	let mut current_config = extnet.config_v4().unwrap();
	current_config.dns_servers.clear();
	current_config
		.dns_servers
		.push(Ipv4Address([1, 1, 1, 1]))
		.unwrap();
	current_config
		.dns_servers
		.push(Ipv4Address([94, 16, 114, 254]))
		.unwrap();
	extnet.set_config_v4(ConfigV4::Static(current_config));

	LogSeverity::Info.log(
		unsafe { MONITOR.as_ref().unwrap() },
		"synchronizing time...".into(),
	);

	if let Some(unixtime) = net::get_unixtime(extnet).await {
	} else {
		LogSeverity::Error.log(
			unsafe { MONITOR.as_ref().unwrap() },
			"failed to get time!".into(),
		);
	}

	LogSeverity::Info.log(unsafe { MONITOR.as_ref().unwrap() }, "booted OK".into());

	loop {
		Timer::after(Duration::from_millis(2000)).await;
	}
}
