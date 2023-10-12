/// XXX TODO Kind of a kitchen sink for stubs of test code at the moment, ignore.
/// XXX TODO Just trying to clean up main.rs a bit.

#[cfg(feature = "oro-connect-to-ip")]
async fn connect_to_oro<'a>(
	_stack: &'static Stack<ExtEthDriver>,
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

	/*
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
	*/

/*
enum TestSessionError {
	EmNet(embassy_net::tcp::Error),
	WriteAll(embedded_io_async::WriteAllError<embassy_net::tcp::Error>),
	ReadExact(embedded_io_async::ReadExactError<embassy_net::tcp::Error>),
}

impl defmt::Format for TestSessionError {
	fn format(&self, fmt: defmt::Formatter) {
		match self {
			TestSessionError::EmNet(err) => defmt::Format::format(err, fmt),
			TestSessionError::WriteAll(err) => defmt::Format::format(err, fmt),
			TestSessionError::ReadExact(err) => defmt::Format::format(err, fmt),
		}
	}
}


impl From<embassy_net::tcp::Error> for TestSessionError {
	fn from(value: embassy_net::tcp::Error) -> Self {
		TestSessionError::EmNet(value)
	}
}

impl From<embedded_io_async::WriteAllError<embassy_net::tcp::Error>> for TestSessionError {
	fn from(value: embedded_io_async::WriteAllError<embassy_net::tcp::Error>) -> Self {
		TestSessionError::WriteAll(value)
	}
}

impl From<embedded_io_async::ReadExactError<embassy_net::tcp::Error>> for TestSessionError {
	fn from(value: embedded_io_async::ReadExactError<embassy_net::tcp::Error>) -> Self {
		TestSessionError::ReadExact(value)
	}
}


async fn run_test_session<'a, RNG: uc::Rng>(
	rng: &mut RNG,
	sock: &mut TcpSocket<'a>,
) -> Result<(), TestSessionError> {
	// Generate key
	let mut private_key = [0u8; 32];
	rng.fill_bytes(&mut private_key);
	let private_key = curve25519::curve25519_sk(private_key);
	let public_key = curve25519::curve25519_pk(private_key);

	sock.write_all(&public_key[..]).await?;

	let mut their_public_key = [0u8; 32];
	sock.read_exact(&mut their_public_key).await?;

	let key = curve25519::curve25519(private_key, their_public_key);

	let enc = Aes256Enc::new_from_slice(&key[..]).unwrap();
	let dec = Aes256Dec::new_from_slice(&key[..]).unwrap();

	debug!("encryption key negotiated");

	// XXX TODO
	let mut block: [u8; 16] = [
		b'H', b'i', b',', b' ', b'O', b'r', b'o', b'!', 0, 0, 0, 0, 0, 0, 0, 0,
	];
	use aes::cipher::BlockEncrypt;
	enc.encrypt_block((&mut block).into());
	sock.write_all(&block[..]).await?;
	debug!("WROTE HELLO");
	Timer::after(Duration::from_millis(5000)).await;

	Ok(())
}

*/

		/*
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

				let r = run_test_session(&mut rng, &mut sock).await;

				unsafe {
					MONITOR.as_ref().unwrap().borrow_mut().set_scene(Scene::Log);
				}

				if let Err(err) = r {
					error!("error with test session socket: {:?}", err);
					LogSeverity::Error.log(
						unsafe { MONITOR.as_ref().unwrap() },
						"test session failure".into(),
					);
				}

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
		*/


/*
#[deprecated]
async fn negotiate_with_sut<E: RawEthernetDriver>(eth: &mut E) {
	use smoltcp::wire;

	let mut buffer = [0u8; 4096];

	let length = {
		let mut eth_frame = wire::EthernetFrame::new_checked(&mut buffer[..]).unwrap();
		eth_frame.set_src_addr(wire::EthernetAddress(eth.address()));
		eth_frame.set_dst_addr(wire::EthernetAddress([0x33, 0x33, 0x00, 0x00, 0x00, 0x02]));
		eth_frame.set_ethertype(wire::EthernetProtocol::Ipv6);
		let eth_payload_len = eth_frame.payload_mut().len();

		wire::EthernetFrame::<&mut [u8]>::header_len() + {
			// IPv4 mapped 10.0.0.1
			let src_addr =
				wire::Ipv6Address([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0x0a, 0, 0, 0x01]);
			// All nodes address (ff02::1)
			// https://www.menandmice.com/blog/ipv6-reference-multicast#well-known-ipv6-multicast-addresses
			let dst_addr =
				wire::Ipv6Address([0xff, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01]);

			let mut ipv6_packet = wire::Ipv6Packet::new_checked(eth_frame.payload_mut()).unwrap();
			ipv6_packet.set_src_addr(src_addr);
			ipv6_packet.set_dst_addr(dst_addr);
			ipv6_packet.set_hop_limit(255);
			ipv6_packet.set_version(6);
			ipv6_packet.set_next_header(wire::IpProtocol::Icmpv6);
			ipv6_packet.set_payload_len((eth_payload_len - ipv6_packet.header_len()) as u16);

			let icmp_len = {
				let mut icmpv6_packet =
					wire::Icmpv6Packet::new_unchecked(ipv6_packet.payload_mut());

				icmpv6_packet.set_msg_type(wire::Icmpv6Message::RouterAdvert);
				// Managed tells the peer that DHCP is available
				// https://www.arubanetworks.com/techdocs/AOS-CX/10.07/HTML/5200-7864/Content/Chp_IPv6_RA/IPv6_RA_cmds/ipv-nd-ra-man-con-fla-10.htm
				icmpv6_packet.set_router_flags(wire::NdiscRouterFlags::MANAGED);
				icmpv6_packet.set_router_lifetime(smoltcp::time::Duration::from_secs(10 * 60));

				icmpv6_packet.header_len()
			};

			ipv6_packet.set_payload_len(icmp_len as u16);

			// Must occur after packet is constructed
			{
				let mut icmpv6_packet =
					wire::Icmpv6Packet::new_unchecked(ipv6_packet.payload_mut());
				icmpv6_packet.fill_checksum(
					&wire::IpAddress::Ipv6(src_addr),
					&wire::IpAddress::Ipv6(dst_addr),
				);
			}

			ipv6_packet.total_len()
		}
	};

	let packet = &buffer[..length];
	eth.send(packet).await;
}
*/

/// The port that the Oro Link CI/CD
//const ORO_CICD_PORT: u16 = 1337;


