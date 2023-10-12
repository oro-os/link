#![no_std]
#![no_main]
#![feature(
	type_alias_impl_trait,
	core_intrinsics,
	byte_slice_trim_ascii,
	async_fn_in_trait,
	trait_alias
)]

mod chip;
mod font;
mod net;
mod service;
mod uc;

use core::cell::RefCell;
#[cfg(not(test))]
use core::panic::PanicInfo;
use defmt::{debug, error, info};
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use static_cell::make_static;
use uc::{LogSeverity, Monitor as _, PowerState, Rng, Scene, SystemUnderTest};

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
type SysEthDriver = impl uc::RawEthernetDriver;
type ImplDebugLed = impl uc::DebugLed;

#[embassy_executor::task]
async fn net_stack_task(stack: &'static Stack<ExtEthDriver>) {
	stack.run().await;
}

type Monitor = impl uc::Monitor;
static mut MONITOR: Option<RefCell<Monitor>> = None;

#[embassy_executor::task]
async fn monitor_task() {
	let monitor = unsafe { MONITOR.as_ref().unwrap() };
	service::monitor::run(monitor).await
}

#[embassy_executor::task]
async fn debug_led_task(debug_led: ImplDebugLed) {
	service::debug_led::run(debug_led).await
}

#[embassy_executor::task]
async fn pxe_broker_task(token: service::pxe::BrokerToken, driver: SysEthDriver) {
	service::pxe::run_broker(token, driver).await
}

#[embassy_executor::task]
async fn pxe_icmp_task(token: service::pxe::IcmpToken) {
	service::pxe::run_icmp(token).await
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) {
	let (
		debug_led,
		mut system,
		monitor,
		exteth,
		syseth,
		_wall_clock,
		mut rng,
		_syscom_tx,
		_syscom_rx,
		packet_tracer,
	) = uc::init(&spawner).await;

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
		let seed = rng.next_u64();
		let config = embassy_net::Config::dhcpv4(Default::default());

		&*make_static!(Stack::new(
			exteth,
			config,
			make_static!(embassy_net::StackResources::<16>::new()),
			seed,
		))
	};

	let syseth = RawEthernetCaptureDriver(syseth, packet_tracer);
	let pxe_tokens = service::pxe::init_pxe();

	spawner.spawn(net_stack_task(extnet)).unwrap();
	spawner.spawn(monitor_task()).unwrap();
	spawner.spawn(debug_led_task(debug_led)).unwrap();
	spawner
		.spawn(pxe_broker_task(pxe_tokens.broker_token, syseth))
		.unwrap();
	spawner.spawn(pxe_icmp_task(pxe_tokens.icmp_token)).unwrap();

	// XXX TODO DEBUG
	debug!("booting the system");
	system.transition_power_state(PowerState::On);
	system.power();
	debug!("system booted");

	loop {
		Timer::after(Duration::from_millis(5000)).await;
	}
}

struct RawEthernetCaptureDriver<D: uc::RawEthernetDriver, P: uc::PacketTracer>(D, P);

impl<D: uc::RawEthernetDriver, P: uc::PacketTracer> uc::RawEthernetDriver
	for RawEthernetCaptureDriver<D, P>
{
	#[inline]
	fn address(&self) -> [u8; 6] {
		self.0.address()
	}

	async fn try_recv(&mut self, buf: &mut [u8]) -> Option<usize> {
		if let Some(count) = self.0.try_recv(buf).await {
			let pkt = &buf[..count];
			self.1.trace_packet(pkt).await;
			Some(count)
		} else {
			None
		}
	}

	async fn send(&mut self, buf: &[u8]) {
		self.1.trace_packet(buf).await;
		self.0.send(buf).await
	}

	#[inline]
	fn is_link_up(&mut self) -> bool {
		self.0.is_link_up()
	}
}
