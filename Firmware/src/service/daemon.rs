use crate::uc;
use aes::{cipher::KeyInit, Aes256Dec, Aes256Enc};
use defmt::{debug, error, info, trace, warn};
use embassy_net::{driver::Driver, tcp::TcpSocket, ConfigV4, Ipv4Address, Stack};
use embassy_time::{Duration, Timer};
use embedded_io_async::{Read, Write};

const ORO_CICD_PORT: u16 = 1337;

pub async fn run<D: Driver + 'static, R: uc::Rng>(stack: &Stack<D>, mut rng: R) -> ! {
	if !stack.is_link_up() {
		warn!("net: link not up; will wait until it is before installing DNS server");
		stack.wait_config_up().await;
	}

	trace!("net: installing setting OpenNIC + CloudFlare dns servers");
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

	loop {
		if !stack.is_link_up() {
			warn!("net: link not up; will wait until it is before connecting to daemon");
			stack.wait_config_up().await;
		}

		let mut tx_buf = [0u8; 2048];
		let mut rx_buf = [0u8; 2048];
		let mut sock = TcpSocket::new(stack, &mut rx_buf[..], &mut tx_buf[..]);
		sock.set_timeout(Some(Duration::from_secs(5)));
		sock.set_keep_alive(Some(Duration::from_secs(2)));
		sock.set_hop_limit(None);

		info!("net: initializing connection to daemon...");
		if connect_to_oro(stack, &mut sock).await.is_err() {
			warn!("net: failed to connect to daemon; retrying after 10s...");
			Timer::after(Duration::from_secs(10)).await;
			continue;
		}

		info!("net: connected to Oro daemon, key beginning negotiation...");

		let mut private_key = [0u8; 32];
		rng.fill_bytes(&mut private_key);
		let private_key = curve25519::curve25519_sk(private_key);
		let public_key = curve25519::curve25519_pk(private_key);

		if let Err(err) = sock.write_all(&public_key[..]).await {
			error!(
				"net: failed to write public key to socket; retrying in 10s: {:?}",
				err
			);
			Timer::after(Duration::from_secs(10)).await;
			continue;
		}

		let mut their_public_key = [0u8; 32];
		if let Err(err) = sock.read_exact(&mut their_public_key).await {
			error!(
				"net: failed to read remote public key from socket; retrying in 10s: {:?}",
				err
			);
			Timer::after(Duration::from_secs(10)).await;
			continue;
		}

		let key = curve25519::curve25519(private_key, their_public_key);

		let enc = Aes256Enc::new_from_slice(&key[..]).unwrap();
		let dec = Aes256Dec::new_from_slice(&key[..]).unwrap();

		debug!("encryption key negotiated");

		Timer::after(Duration::from_secs(500)).await; // XXX DEBUG

		/*
			// XXX TODO
			let mut block: [u8; 16] = [
				b'H', b'i', b',', b' ', b'O', b'r', b'o', b'!', 0, 0, 0, 0, 0, 0, 0, 0,
			];
			use aes::cipher::BlockEncrypt;
			enc.encrypt_block((&mut block).into());
			sock.write_all(&block[..]).await?;
		*/
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
		"oro link firmware was built with 'oro-connect-to-ip'; skipping oro.dyn resolution and instead connecting to {:?}",
		ip_bytes
	);

	let ip = Ipv4Address::new(ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]);

	if let Err(err) = sock.connect((ip, ORO_CICD_PORT)).await {
		error!("failed to connect to {:?}: {:?}", ip, err);
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
				error!("net: failed to fetch oro.dyn address: resolved address zero count");
				return Err(());
			}

			a[0]
		}
		Err(err) => {
			error!("net: failed to fetch oro.dyn address: {:?}", err);
			return Err(());
		}
	};

	info!("oro.dyn resolved to {:?}; connecting...", oro_dyn);

	if let Err(err) = sock.connect((oro_dyn, ORO_CICD_PORT)).await {
		error!("failed to connect to oro.dyn ({:?}): {:?}", oro_dyn, err);

		return Err(());
	}

	Ok(())
}
