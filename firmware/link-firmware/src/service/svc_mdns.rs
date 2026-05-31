use embassy_net::{
	IpEndpoint, Ipv4Address, Stack,
	udp::{PacketMetadata, UdpSocket},
};
use embassy_time::{Duration, Timer};
use smoltcp::wire::Ipv4Address as SmolIpv4;
use static_cell::StaticCell;

const MDNS_PORT: u16 = 5353;
const MDNS_MULTICAST: Ipv4Address = Ipv4Address::new(224, 0, 0, 251);

pub struct Config {
	pub stack: Stack<'static>,
}

#[embassy_executor::task]
pub async fn run(config: Config) -> ! {
	let Config { stack } = config;

	defmt::debug!("waiting for network...");
	stack.wait_config_up().await;
	defmt::debug!("network is up; looking for area controllers...");

	// static RX_BUF: StaticCell<[u8; 1024]> = StaticCell::new();
	// static TX_BUF: StaticCell<[u8; 512]> = StaticCell::new();
	// let mut rx_meta = [PacketMetadata::EMPTY; 4];
	// let rx_buf = RX_BUF.init([0u8; 1024]);
	// let mut tx_meta = [PacketMetadata::EMPTY; 2];
	// let tx_buf = TX_BUF.init([0u8; 512]);

	// let mut socket = UdpSocket::new(
	// 	stack,
	// 	&mut rx_meta,
	// 	rx_buf,
	// 	&mut tx_meta,
	// 	tx_buf,
	//);

	// socket.bind(MDNS_PORT).expect("failed to bind MDNS socket");
	// stack.join_multicast_group(MDNS_MULTICAST).expect("failed to join multicast group");

	let dns = embassy_net::dns::DnsSocket::new(stack);
	let result = dns
		.query("oro.sh", embassy_net::dns::DnsQueryType::A)
		.await
		.expect("DNS query failed");
	defmt::warn!("DNS query result: oro.sh: {:?}", result);
	let result = dns
		.query(
			"_matrix._tcp.matrix.org",
			embassy_net::dns::DnsQueryType::Srv,
		)
		.await
		.expect("DNS query failed");
	defmt::warn!("DNS query result: _matrix._tcp.matrix.org: {:?}", result);

	loop {
		//// Build a multi-question query
		// let mut buf = [0u8; 512];
		// let len = build_multi_question_mdns_query(&mut buf);

		// let endpoint = IpEndpoint::new(
		//    MDNS_MULTICAST.into(),
		//    MDNS_PORT,
		//);

		//// Send the query
		// match socket.send_to(&buf[..len], endpoint).await {
		//    Ok(_) => defmt::info!("mDNS multi-question query sent"),
		//    Err(e) => defmt::warn!("Send error: {:?}", e),
		//}

		Timer::after(Duration::from_secs(10)).await;
	}
}

// fn build_multi_question_mdns_query(buf: &mut [u8]) -> usize {
//    use smoltcp::wire::{
//        DnsPacket, DnsRepr, DnsFlags, DnsOpcode, DnsRcode,
//        DnsQuestion, DnsQueryType as DnsType, // Note: it's DnsQueryType in some versions
//    };
//
//    let mut packet = match DnsPacket::new_checked(buf) {
//        Ok(p) => p,
//        Err(_) => return 0,
//    };
//
//    let repr = DnsRepr {
//        transaction_id: 0,
//        opcode: DnsOpcode::Query,
//        flags: DnsFlags::empty(),
//        question: DnsQuestion {           // Only ONE question supported in Repr
//            name: b"mydevice.local",
//            type_: DnsType::
//        },
//    };
//
//    repr.emit(&mut packet);
// 	packet.payload().len()
//}
//
