use crate::{
	command::{Command, CommandReceiver, CommandSender},
	uc,
};
use defmt::{debug, error, info, trace, warn};
use embassy_futures::select::{select, Either};
use embassy_net::{
	driver::Driver,
	tcp::{TcpReader, TcpSocket, TcpWriter},
	ConfigV4, Ipv4Address, Stack,
};
use embassy_time::{Duration, Timer};
use embedded_io_async::{Read, Write};
use link_protocol::{
	channel::{PacketReceiver, PacketSender},
	Error as LinkPacketError, Packet,
};

const ORO_CICD_PORT: u16 = 1337;

pub async fn run<D: Driver + 'static, R: uc::Rng>(
	stack: &Stack<D>,
	mut rng: R,
	broker_sender: CommandSender<32>,
	daemon_receiver: CommandReceiver<16>,
) -> ! {
	static mut TX_BUF: [u8; 2048] = [0u8; 2048];
	static mut RX_BUF: [u8; 2048] = [0u8; 2048];
	let mut sock = TcpSocket::new(stack, unsafe { &mut RX_BUF[..] }, unsafe {
		&mut TX_BUF[..]
	});

	loop {
		if !stack.is_link_up() {
			warn!("daemon: link not up; will wait until it is before connecting to daemon");
			stack.wait_config_up().await;
		}

		trace!("daemon: installing setting OpenNIC + CloudFlare dns servers");
		let mut current_config = stack.config_v4().unwrap();
		current_config.dns_servers.clear();
		current_config.dns_servers.clear();
		current_config
			.dns_servers
			.push(Ipv4Address([1, 1, 1, 1]))
			.unwrap();
		current_config
			.dns_servers
			.push(Ipv4Address([94, 16, 114, 254])) // TODO: pull from an opennic.oro.sh record.
			.unwrap();
		stack.set_config_v4(ConfigV4::Static(current_config));

		sock.set_timeout(Some(Duration::from_secs(5)));
		sock.set_keep_alive(Some(Duration::from_secs(2)));
		sock.set_hop_limit(None);

		info!("daemon: initializing connection to daemon");
		if connect_to_oro(stack, &mut sock).await.is_err() {
			warn!("daemon: failed to connect to daemon; retrying after 10s...");
			Timer::after(Duration::from_secs(10)).await;
			continue;
		}

		info!("daemon: negotiating daemon session");
		let (receiver, sender) = sock.split();
		todo!("create channel");

		debug!("daemon: encryption key negotiated");
		todo!("select between channel receive / socket receive");

		sock.abort();
	}
}

#[cfg(feature = "oro-connect-to-ip")]
async fn connect_to_oro<'a, D: Driver + 'static>(
	_stack: &Stack<D>,
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
		"daemon: oro link firmware was built with 'oro-connect-to-ip'; skipping oro.dyn resolution and instead connecting to {:?}",
		ip_bytes
	);

	let ip = Ipv4Address::new(ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]);

	if let Err(err) = sock.connect((ip, ORO_CICD_PORT)).await {
		error!("daemon: failed to connect to {:?}: {:?}", ip, err);
		return Err(());
	}

	Ok(())
}

#[cfg(not(feature = "oro-connect-to-ip"))]
async fn connect_to_oro<'a, D: Driver + 'static>(
	stack: &Stack<D>,
	sock: &mut TcpSocket<'a>,
) -> Result<(), ()> {
	let oro_dyn = match stack
		.dns_query("oro.dyn", embassy_net::dns::DnsQueryType::A)
		.await
	{
		Ok(a) => {
			if a.is_empty() {
				error!("daemon: failed to fetch oro.dyn address: resolved address zero count");
				return Err(());
			}

			a[0]
		}
		Err(err) => {
			error!("daemon: failed to fetch oro.dyn address: {:?}", err);
			return Err(());
		}
	};

	info!("daemon: oro.dyn resolved to {:?}; connecting...", oro_dyn);

	if let Err(err) = sock.connect((oro_dyn, ORO_CICD_PORT)).await {
		error!(
			"daemon: failed to connect to oro.dyn ({:?}): {:?}",
			oro_dyn, err
		);

		return Err(());
	}

	Ok(())
}
