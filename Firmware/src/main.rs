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
use uc::{LogSeverity, Monitor as _, PowerState, Rng, Scene, SystemUnderTest, UniqueId};

#[defmt::panic_handler]
fn defmt_panic() -> ! {
	#[allow(clippy::empty_loop)]
	loop {}
}

#[cfg(not(test))]
#[panic_handler]
fn panic(panic: &PanicInfo<'_>) -> ! {
	let line = panic.location().map(|l| l.file()).unwrap_or("?");
	let col = panic.location().map(|l| l.line()).unwrap_or(0);

	if let Some(s) = panic.payload().downcast_ref::<&str>() {
		error!("PANIC @ {}:{}: {}", line, col, s);
	} else {
		error!("PANIC @ {}:{}: <unknown>", line, col);
	}

	// TODO cortex_m::SCB::sys_reset();
	#[allow(clippy::empty_loop)]
	loop {}
}

type ExtEthernetDriver = impl uc::EthernetDriver;
type SysEthernetDriver = impl uc::EthernetDriver;
type ImplDebugLed = impl uc::DebugLed;
type ImplMonitor = impl uc::Monitor;
type ImplWallClock = impl uc::WallClock;
type ImplRng = impl uc::Rng;
type ImplUniqueId = impl uc::UniqueId;
static mut MONITOR: Option<RefCell<ImplMonitor>> = None;

#[embassy_executor::task]
async fn net_ext_stack_task(stack: &'static Stack<ExtEthernetDriver>) -> ! {
	stack.run().await
}

#[embassy_executor::task]
async fn net_sys_stack_task(stack: &'static Stack<SysEthernetDriver>) -> ! {
	stack.run().await
}

#[embassy_executor::task]
async fn monitor_task() -> ! {
	let monitor = unsafe { MONITOR.as_ref().unwrap() };
	service::monitor::run(monitor).await
}

#[embassy_executor::task]
async fn debug_led_task(debug_led: ImplDebugLed) -> ! {
	service::debug_led::run(debug_led).await
}

#[embassy_executor::task]
async fn pxe_task(stack: &'static Stack<SysEthernetDriver>) -> ! {
	service::pxe::run(stack).await
}

#[embassy_executor::task]
async fn tftp_task(stack: &'static Stack<SysEthernetDriver>) -> ! {
	service::tftp::run(stack).await
}

#[embassy_executor::task]
async fn time_task(stack: &'static Stack<ExtEthernetDriver>, wall_clock: ImplWallClock) -> ! {
	service::time::run(stack, wall_clock).await
}

#[embassy_executor::task]
async fn daemon_task(
	stack: &'static Stack<ExtEthernetDriver>,
	rng: ImplRng,
	uid: ImplUniqueId,
) -> ! {
	service::daemon::run(stack, rng, &uid).await
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) {
	let (
		debug_led,
		mut system,
		monitor,
		exteth,
		syseth,
		wall_clock,
		mut rng,
		_syscom_tx,
		_syscom_rx,
		packet_tracer,
		uid,
	) = uc::init(&spawner).await;

	unsafe {
		MONITOR = {
			fn init(monitor: ImplMonitor) -> Option<RefCell<ImplMonitor>> {
				Some(RefCell::new(monitor))
			}
			init(monitor)
		};
	}

	info!(
		"Oro Link x86 booting (version {})",
		env!("CARGO_PKG_VERSION")
	);

	info!("Link UID: {:?}", uid.unique_id());

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

	let link_uid = uid.unique_id();

	spawner.must_spawn(net_ext_stack_task(extnet));
	spawner.must_spawn(net_sys_stack_task(sysnet));
	spawner.must_spawn(monitor_task());
	spawner.must_spawn(debug_led_task(debug_led));
	spawner.must_spawn(pxe_task(sysnet));
	spawner.must_spawn(tftp_task(sysnet));
	spawner.must_spawn(time_task(extnet, wall_clock));
	spawner.must_spawn(daemon_task(extnet, rng, uid));

	loop {
		// XXX DEBUG
		{
			let mut buffer: Vec<u8, 2048> = Vec::new();
			use ::link_protocol::{Deserialize, Serialize};
			let src = ::link_protocol::LinkPacket::LinkOnline {
				uid: link_uid,
				version: env!("CARGO_PKG_VERSION").into(),
			};
			info!("SRC = {:#?}", src);
			let r = {
				let mut writer = VecWriter::new(&mut buffer);
				src.serialize(&mut writer).await
			};
			match r {
				Ok(()) => {
					info!("OK, serialized source: {:?}", &buffer[..]);
					let mut reader = VecReader::new(&buffer);
					match ::link_protocol::LinkPacket::deserialize(&mut reader).await {
						Ok(dst) => {
							info!("OK, deserialized = {:#?}", dst);
						}
						Err(err) => {
							error!("failed to deserialize dst: {:?}", err);
						}
					}
				}
				Err(err) => error!("failed to serialize src: {:?}", err),
			}
		}

		// XXX TODO DEBUG
		debug!("booting the system");
		system.transition_power_state(PowerState::On);
		system.power();

		debug!("system booted; waiting for PXE...");
		Timer::after(Duration::from_millis(30000)).await;

		debug!("pxe boot attempted; shutting down...");
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

/* XXX just some test code */
pub struct VecWriter<'a, const SZ: usize> {
	buffer: &'a mut Vec<u8, SZ>,
}

impl<'a, const SZ: usize> VecWriter<'a, SZ> {
	pub fn new(buffer: &'a mut Vec<u8, SZ>) -> Self {
		VecWriter { buffer }
	}
}

impl<'a, const SZ: usize> link_protocol::Write for VecWriter<'a, SZ> {
	async fn write(&mut self, buf: &[u8]) -> Result<(), link_protocol::Error> {
		if self.buffer.len() + buf.len() <= SZ {
			self.buffer
				.extend_from_slice(buf)
				.map_err(|_| link_protocol::Error::Eof)?;
			Ok(())
		} else {
			Err(link_protocol::Error::Eof)
		}
	}
}

pub struct VecReader<'a, const SZ: usize> {
	buffer: &'a Vec<u8, SZ>,
	pos: usize,
}

impl<'a, const SZ: usize> VecReader<'a, SZ> {
	pub fn new(buffer: &'a Vec<u8, SZ>) -> Self {
		VecReader { buffer, pos: 0 }
	}
}

impl<'a, const SZ: usize> link_protocol::Read for VecReader<'a, SZ> {
	async fn read(&mut self, buf: &mut [u8]) -> Result<(), link_protocol::Error> {
		if self.pos < self.buffer.len() {
			let available = self.buffer.len() - self.pos;
			let to_copy = core::cmp::min(available, buf.len());
			let src_slice = &self.buffer[self.pos..self.pos + to_copy];
			buf[0..to_copy].copy_from_slice(src_slice);
			self.pos += to_copy;
			Ok(())
		} else {
			Err(link_protocol::Error::Eof)
		}
	}
}
