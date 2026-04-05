use core::net::{Ipv4Addr, Ipv6Addr};

use edge_mdns::domain::base::Ttl;
use edge_nal::UdpSplit;
use embassy_futures::select::Either;
use embassy_net::{IpListenEndpoint, Stack};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::Timer;
use static_cell::StaticCell;

pub struct Config {
	pub stack: Stack<'static>,
}

#[embassy_executor::task]
pub async fn run(config: Config) -> ! {
	let mqtt = match wait_for_mqtt(config.stack).await {
		Ok(mqtt) => mqtt,
		Err(err) => {
			defmt::error!("MQTT initialization failed; resetting in 5s: {:?}", err);
			Timer::after_secs(5).await;
			// SAFETY: MQTT failures must reset.
			unsafe {
				crate::reset();
			}
		}
	};

	defmt::info!("MQTT session established");

	loop {
		Timer::after_secs(10).await;
	}
}

/// The error type when establishing a connection
#[derive(defmt::Format)]
pub enum Error {
	/// An error occurred with the mDNS resolver
	Mdns(edge_mdns::io::MdnsIoError<edge_nal_embassy::UdpError>),
	/// The network device went down before we could wait for mDNS discovery.
	LinkDown,
	/// The mDNS responder stopped unexpectedly while waiting for a connection.
	MdnsStopped,
	/// An error occurred with the TCP listener
	Accept(embassy_net::tcp::AcceptError),
}

/// # Panics
/// Can only be called once. The board should reset if the connection
/// is lost.
pub async fn wait_for_mqtt<'stack>(stack: Stack<'stack>) -> Result<(), Error> {
	defmt::debug!("waiting for stack to be configured");
	stack.wait_config_up().await;

	let Some(our_endpoint) = stack.config_v4() else {
		defmt::error!("the link returned None for the ipv4 config");
		return Err(Error::LinkDown);
	};

	let embassy_net::HardwareAddress::Ethernet(mac) = stack.hardware_address() else {
		defmt::error!("the link's stack hardware address is not a MAC address");
		return Err(Error::LinkDown);
	};

	static TX_BUFFER: StaticCell<[u8; 4096]> = StaticCell::new();
	static RX_BUFFER: StaticCell<[u8; 4096]> = StaticCell::new();

	let tx_buffer = TX_BUFFER.init([0; 4096]);
	let rx_buffer = RX_BUFFER.init([0; 4096]);

	defmt::debug!("creating listener for MQTT connection");
	let mut listener = embassy_net::tcp::TcpSocket::new(stack, rx_buffer, tx_buffer);

	defmt::debug!("creating mDNS resolver");
	let mdns_buffers = edge_nal_embassy::UdpBuffers::<1>::new();
	let mdns_stack = edge_nal_embassy::Udp::new(stack, &mdns_buffers);

	let (mdns_recv_buf, mdns_send_buf) = (
		edge_mdns::buf::VecBufAccess::<NoopRawMutex, 1500>::new(),
		edge_mdns::buf::VecBufAccess::<NoopRawMutex, 1500>::new(),
	);

	let mut mdns_socket = edge_mdns::io::bind(
		&mdns_stack,
		edge_mdns::io::DEFAULT_SOCKET,
		Some(Ipv4Addr::UNSPECIFIED),
		Some(0),
	)
	.await
	.map_err(Error::Mdns)?;

	let (recv, send) = mdns_socket.split();

	let name = crate::unique_id();

	let host = edge_mdns::host::Host {
		hostname: "orolink",
		ipv4:     our_endpoint.address.address(),
		ipv6:     Ipv6Addr::UNSPECIFIED,
		ttl:      Ttl::from_secs(5),
	};

	let service = edge_mdns::host::Service {
		name,
		priority: 1,
		weight: 5,
		service: "_orolink",
		protocol: "_tcp",
		port: 1883,
		service_subtypes: &[],
		txt_kvs: &[],
	};

	// A way to notify the mDNS responder that the data in `Host` had changed
	// We don't use it in this example, because the data is hard-coded
	let signal = embassy_sync::signal::Signal::<NoopRawMutex, _>::new();

	let mdns = edge_mdns::io::Mdns::new(
		Some(Ipv4Addr::UNSPECIFIED),
		Some(0),
		recv,
		send,
		mdns_recv_buf,
		mdns_send_buf,
		crate::rand::rng(),
		&signal,
	);

	// Wait for a connection to be established, and then return.
	let r = embassy_futures::select::select(
		mdns.run(edge_mdns::HostAnswersMdnsHandler::new(
			edge_mdns::host::ServiceAnswers::new(&host, &service),
		)),
		listener.accept(IpListenEndpoint {
			addr: None,
			port: 1883,
		}),
	)
	.await;

	match r {
		Either::First(Ok(_)) => {
			defmt::info!("mDNS responder stopped unexpectedly");
			return Err(Error::MdnsStopped);
		}
		Either::First(Err(err)) => {
			defmt::error!("mDNS responder error: {:?}", err);
			return Err(Error::Mdns(err));
		}
		Either::Second(Ok(_)) => {
			defmt::info!("MQTT client connected");
		}
		Either::Second(Err(err)) => {
			defmt::error!("TCP listener error: {:?}", err);
			return Err(Error::Accept(err));
		}
	}

	loop {
		// TODO
		Timer::after_secs(10).await;
	}

	Ok(())
}
