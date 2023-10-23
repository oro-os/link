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
mod command;
mod font;
mod service;
mod uc;

use command::{Command, CommandChannel, CommandReceiver, CommandSender};
use core::cell::RefCell;
#[cfg(not(test))]
use core::{panic::PanicInfo, task::Context};
use defmt::{debug, error, info, warn};
use embassy_executor::Spawner;
use embassy_net::{
	driver::{Capabilities, HardwareAddress, LinkState, RxToken, TxToken},
	Ipv4Address, Ipv4Cidr, Stack, StaticConfigV4,
};
use embassy_time::{Duration, Timer};
use heapless::Vec;
use link_protocol::{self as proto, Packet};
use static_cell::make_static;
use uc::{Monitor as _, ResetManager, Rng, Scene, UniqueId};

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
async fn monitor_task(receiver: CommandReceiver<4>) -> ! {
	let monitor = unsafe { MONITOR.as_ref().unwrap() };
	service::monitor::run(monitor, receiver).await
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
	broker_sender: CommandSender<32>,
	daemon_receiver: CommandReceiver<16>,
) -> ! {
	service::daemon::run(stack, rng, broker_sender, daemon_receiver).await
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
	let (
		debug_led,
		_system,
		monitor,
		exteth,
		syseth,
		wall_clock,
		mut rng,
		_syscom_tx,
		_syscom_rx,
		packet_tracer,
		uid,
		rst,
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
		"oro link x86 booting (version {})",
		env!("CARGO_PKG_VERSION")
	);

	info!("link uid: {:?}", uid.unique_id());

	unsafe {
		MONITOR.as_ref().unwrap().borrow_mut().set_scene(Scene::Log);
	}

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

	static mut BROKER_CHANNEL: CommandChannel<32> = CommandChannel::new();
	static mut DAEMON_CHANNEL: CommandChannel<16> = CommandChannel::new();
	static mut MONITOR_CHANNEL: CommandChannel<4> = CommandChannel::new();

	let broker_receiver = unsafe { BROKER_CHANNEL.receiver() };
	let broker_sender = unsafe { BROKER_CHANNEL.sender() };
	let daemon_sender = unsafe { DAEMON_CHANNEL.sender() };
	let daemon_receiver = unsafe { DAEMON_CHANNEL.receiver() };
	let monitor_sender = unsafe { MONITOR_CHANNEL.sender() };
	let monitor_receiver = unsafe { MONITOR_CHANNEL.receiver() };

	spawner.must_spawn(net_ext_stack_task(extnet));
	spawner.must_spawn(net_sys_stack_task(sysnet));
	spawner.must_spawn(monitor_task(monitor_receiver));
	spawner.must_spawn(debug_led_task(debug_led));
	spawner.must_spawn(pxe_task(sysnet));
	spawner.must_spawn(tftp_task(sysnet));
	spawner.must_spawn(time_task(extnet, wall_clock));
	spawner.must_spawn(daemon_task(extnet, rng, broker_sender, daemon_receiver));

	loop {
		match broker_receiver.receive().await {
			Command::Packet(Packet::SetScene(scene)) => {
				monitor_sender
					.send(Command::SetScene(match scene {
						proto::Scene::Log => uc::Scene::Log,
						proto::Scene::Logo => uc::Scene::OroLogo,
						proto::Scene::Test => uc::Scene::Test,
						unknown => {
							warn!(
								"daemon: requested to switch to unknown scene: {:?}",
								unknown
							);
							continue;
						}
					}))
					.await
			}
			Command::Packet(Packet::Log(entry)) => {
				monitor_sender
					.send(Command::Log(match entry {
						proto::LogEntry::Info(msg) => uc::LogSeverity::Info.make(msg),
						proto::LogEntry::Warn(msg) => uc::LogSeverity::Warn.make(msg),
						proto::LogEntry::Error(msg) => uc::LogSeverity::Error.make(msg),
						unknown => {
							warn!(
								"daemon: requested to log to monitor with unknown level: {:?}",
								unknown
							);
							continue;
						}
					}))
					.await
			}
			Command::Packet(Packet::SetMonitorStandby(standby)) => {
				monitor_sender.send(Command::SetStandby(standby)).await
			}
			Command::Packet(Packet::StartTestSession {
				total_tests,
				author,
				title,
				ref_id,
			}) => {
				monitor_sender
					.send(Command::StartTestSession {
						total_tests: total_tests as usize,
						author,
						title,
						ref_id,
					})
					.await
			}
			Command::DaemonConnected => {
				debug!("broker: telling daemon we're online");
				daemon_sender
					.send(Command::Packet(Packet::LinkOnline {
						uid: uid.unique_id(),
						version: env!("CARGO_PKG_VERSION").into(),
					}))
					.await;
			}
			Command::SetScene(scene) => monitor_sender.send(Command::SetScene(scene)).await,
			Command::Log(entry) => monitor_sender.send(Command::Log(entry)).await,
			#[allow(clippy::diverging_sub_expression)]
			Command::Packet(Packet::ResetLink) | Command::Reset => {
				warn!("!!! LINK WILL RESET IN 50ms !!!");
				Timer::after(Duration::from_millis(50)).await;
				rst.reset();
				#[allow(unreachable_code)]
				{
					unreachable!();
				}
			}
			unknown => {
				warn!("broker: unexpected command: {:?}", unknown);
			}
		}
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
