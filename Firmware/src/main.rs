#![no_std]
#![no_main]
#![feature(type_alias_impl_trait, core_intrinsics, byte_slice_trim_ascii)]

mod chip;
mod font;
mod net;
mod uc;

use core::cell::RefCell;
#[cfg(not(test))]
use core::panic::PanicInfo;
use defmt::{error, info, warn};
use embassy_executor::Spawner;
use embassy_net::{tcp::TcpSocket, ConfigV4, Ipv4Address, Stack};
use embassy_time::{Duration, Instant, Timer};
use static_cell::make_static;
use uc::{DebugLed, LogSeverity, Monitor as _, Rng, Scene, WallClock};

/// The port that the Oro Link CI/CD
const ORO_CICD_PORT: u16 = 1337;

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

#[cfg(feature = "oro-connect-to-ip")]
async fn connect_to_oro<'a>(
	stack: &'static Stack<ExtEthDriver>,
	sock: &mut TcpSocket<'a>,
) -> Result<(), ()> {
	const DEV_IP: &'static str = env!("ORO_CONNECT_TO_IP");

	let mut ip_bytes = [0u8; 4];
	for (i, octet) in DEV_IP
		.split(".")
		.take(4)
		.map(|s| s.parse::<u8>().unwrap())
		.enumerate()
	{
		ip_bytes[i] = octet;
	}

	warn!(
		"oro link firmware was built with 'oro-connect-to-ip'; skipping oro.dyn resolution and instead connecting to {:?}",
		ip_bytes
	);

	let ip = Ipv4Address::new(ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]);

	if let Err(err) = sock.connect((ip, ORO_CICD_PORT)).await {
		error!("failed to connect to {:?}: {:?}", ip, err);

		LogSeverity::Error.log(
			unsafe { MONITOR.as_ref().unwrap() },
			"failed to connect (dev IP)".into(),
		);

		return Err(());
	}

	Ok(())
}

#[cfg(not(feature = "oro-connect-to-ip"))]
async fn connect_to_oro<'a>(
	stack: &'static Stack<ExtEthDriver>,
	sock: &mut TcpSocket<'a>,
) -> Result<(), ()> {
	LogSeverity::Info.log(
		unsafe { MONITOR.as_ref().unwrap() },
		"resolving oro.dyn".into(),
	);

	let oro_dyn = match stack
		.dns_query("oro.dyn", embassy_net::dns::DnsQueryType::A)
		.await
	{
		Ok(a) => {
			if a.is_empty() {
				error!("failed to fetch oro.dyn address: resolved address zero count");
				return Err(());
			}

			a[0]
		}
		Err(err) => {
			error!("failed to fetch oro.dyn address: {:?}", err);
			LogSeverity::Error.log(
				unsafe { MONITOR.as_ref().unwrap() },
				"failed to resolve oro.dyn".into(),
			);
			return Err(());
		}
	};

	info!("oro.dyn resolved to {:?}; connecting...", oro_dyn);

	LogSeverity::Info.log(
		unsafe { MONITOR.as_ref().unwrap() },
		"connecting to oro.dyn...".into(),
	);

	if let Err(err) = sock.connect((oro_dyn, ORO_CICD_PORT)).await {
		error!("failed to connect to oro.dyn ({:?}): {:?}", oro_dyn, err);

		LogSeverity::Error.log(
			unsafe { MONITOR.as_ref().unwrap() },
			"failed to connect".into(),
		);

		return Err(());
	}

	Ok(())
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) {
	let (debug_led, _system, monitor, exteth, mut wall_clock, mut rng) = uc::init(&spawner).await;

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
	}

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
	extnet.set_config_v4(ConfigV4::Static(current_config));

	LogSeverity::Info.log(
		unsafe { MONITOR.as_ref().unwrap() },
		"synchronizing time...".into(),
	);

	if let Some(datetime) = net::get_datetime(extnet).await {
		info!("current datetime: {:#?}", datetime);
		wall_clock.set_datetime(datetime);
	} else {
		LogSeverity::Error.log(
			unsafe { MONITOR.as_ref().unwrap() },
			"failed to get time!".into(),
		);
	}

	let mut current_config = extnet.config_v4().unwrap();
	current_config.dns_servers.clear();
	current_config
		.dns_servers
		.push(Ipv4Address([94, 16, 114, 254]))
		.unwrap();
	extnet.set_config_v4(ConfigV4::Static(current_config));

	LogSeverity::Info.log(unsafe { MONITOR.as_ref().unwrap() }, "booted OK".into());

	let mut tx_buf = [0u8; 2048];
	let mut rx_buf = [0u8; 2048];
	let mut sock = TcpSocket::new(extnet, &mut rx_buf[..], &mut tx_buf[..]);
	sock.set_timeout(Some(Duration::from_secs(5)));
	sock.set_keep_alive(Some(Duration::from_secs(2)));
	sock.set_hop_limit(None);

	loop {
		LogSeverity::Warn.log(
			unsafe { MONITOR.as_ref().unwrap() },
			"starting new test session in 1s".into(),
		);
		Timer::after(Duration::from_millis(1000)).await;

		unsafe {
			MONITOR.as_ref().unwrap().borrow_mut().set_scene(Scene::Log);
		}

		if connect_to_oro(extnet, &mut sock).await.is_err() {
			Timer::after(Duration::from_millis(10000)).await;
			continue;
		}

		info!("connected to oro.dyn");
		LogSeverity::Info.log(
			unsafe { MONITOR.as_ref().unwrap() },
			"connected to oro.dyn".into(),
		);

		Timer::after(Duration::from_millis(1000)).await;

		info!("closing socket to oro.dyn");
		LogSeverity::Info.log(
			unsafe { MONITOR.as_ref().unwrap() },
			"terminating connection to oro.dyn...".into(),
		);

		sock.abort();

		if let Err(err) = sock.flush().await {
			warn!(
				"failed to flush oro.dyn socket after call to abort(); socket may act abnormally: {:?}",
				err
			);
			LogSeverity::Warn.log(
				unsafe { MONITOR.as_ref().unwrap() },
				"failed to close connection!".into(),
			);
			LogSeverity::Warn.log(
				unsafe { MONITOR.as_ref().unwrap() },
				"oro link may need a reset!".into(),
			);
		}

		// Generate key
		//		let mut private_key = [0u8; 32];
		//		rng.fill_bytes(&mut private_key);
		//		let private_key = curve25519::curve25519_sk(private_key);
		//		let public_key = curve25519::curve25519_pk(private_key.clone());
		//
		//		let their_public_key = [0u8; 32]; // XXX TODO
		//
		//		let key = curve25519::curve25519(private_key, their_public_key);
	}
}
