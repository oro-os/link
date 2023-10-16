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
use core::{panic::PanicInfo, task::Context};
use defmt::{debug, error, info};
use embassy_executor::Spawner;
use embassy_net::{
	driver::{Capabilities, HardwareAddress, LinkState, RxToken, TxToken},
	Ipv4Address, Ipv4Cidr, Stack, StaticConfigV4,
};
use embassy_time::{Duration, Timer};
use heapless::Vec;
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

type ExtEthernetDriver = impl uc::EthernetDriver;
type SysEthernetDriver = impl uc::EthernetDriver;
type ImplDebugLed = impl uc::DebugLed;

#[embassy_executor::task]
async fn net_ext_stack_task(stack: &'static Stack<ExtEthernetDriver>) {
	stack.run().await;
}

#[embassy_executor::task]
async fn net_sys_stack_task(stack: &'static Stack<SysEthernetDriver>) {
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

	let sysnet = {
		let seed = rng.next_u64();
		let config = embassy_net::Config::ipv4_static(StaticConfigV4 {
			address: Ipv4Cidr::new(Ipv4Address([10, 0, 0, 1]), 24),
			gateway: None,
			dns_servers: Vec::new(),
		});

		let syseth = EthernetCaptureDriver(syseth, RefCell::new(packet_tracer));

		&*make_static!(Stack::new(
			syseth,
			config,
			make_static!(embassy_net::StackResources::<16>::new()),
			seed,
		))
	};

	spawner.must_spawn(net_ext_stack_task(extnet));
	spawner.must_spawn(net_sys_stack_task(sysnet));
	spawner.must_spawn(monitor_task());
	spawner.must_spawn(debug_led_task(debug_led));

	loop {
		// XXX TODO DEBUG
		debug!("booting the system");
		system.transition_power_state(PowerState::On);
		system.power();
		debug!("system booted; booting PXE...");

		net::pxe::handshake_dhcp(sysnet).await;

		debug!("pxe boot attempted; shutting down...");
		Timer::after(Duration::from_millis(3000)).await;
		system.transition_power_state(PowerState::Off);

		debug!("shut down; restarting in 8s...");
		Timer::after(Duration::from_millis(8000)).await;
	}
}

struct EthernetCaptureDriver<D: uc::EthernetDriver, P: uc::PacketTracer>(D, RefCell<P>);

struct EthernetCaptureTxToken<'a, T: TxToken, P: uc::PacketTracer>(T, &'a RefCell<P>);

struct EthernetCaptureRxToken<'a, T: RxToken, P: uc::PacketTracer>(T, &'a RefCell<P>);

impl<D: uc::EthernetDriver, P: uc::PacketTracer> embassy_net::driver::Driver
	for EthernetCaptureDriver<D, P>
{
	type RxToken<'a> = EthernetCaptureRxToken<'a, D::RxToken<'a>, P> where P: 'a, D: 'a;
	type TxToken<'a> = EthernetCaptureTxToken<'a, D::TxToken<'a>, P> where P: 'a, D: 'a;

	fn receive(&mut self, cx: &mut Context<'_>) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
		let res = self.0.receive(cx);
		if let Some((rxt, txt)) = res {
			Some((
				EthernetCaptureRxToken(rxt, &self.1),
				EthernetCaptureTxToken(txt, &self.1),
			))
		} else {
			None
		}
	}

	fn transmit(&mut self, cx: &mut Context<'_>) -> Option<Self::TxToken<'_>> {
		match self.0.transmit(cx) {
			None => None,
			Some(txt) => Some(EthernetCaptureTxToken(txt, &self.1)),
		}
	}

	#[inline]
	fn link_state(&mut self, cx: &mut Context<'_>) -> LinkState {
		self.0.link_state(cx)
	}

	#[inline]
	fn capabilities(&self) -> Capabilities {
		self.0.capabilities()
	}

	#[inline]
	fn hardware_address(&self) -> HardwareAddress {
		self.0.hardware_address()
	}
}

impl<'a, T: TxToken, P: uc::PacketTracer> TxToken for EthernetCaptureTxToken<'a, T, P> {
	fn consume<R, F>(self, len: usize, f: F) -> R
	where
		F: FnOnce(&mut [u8]) -> R,
	{
		self.0.consume(len, |buf| {
			let r = f(buf);
			self.1.borrow_mut().trace_packet(buf);
			r
		})
	}
}

impl<'a, T: RxToken, P: uc::PacketTracer> RxToken for EthernetCaptureRxToken<'a, T, P> {
	fn consume<R, F>(self, f: F) -> R
	where
		F: FnOnce(&mut [u8]) -> R,
	{
		self.0.consume(|buf| {
			self.1.borrow_mut().trace_packet(buf);
			f(buf)
		})
	}
}
