use core::net::{Ipv4Addr, Ipv6Addr};

use edge_nal::{
	UdpSplit,
	io::{Read, Write},
};
use embassy_futures::select::Either;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::Timer;
use static_cell::StaticCell;

#[derive(defmt::Format)]
enum Error {
	/// An error occurred with the QUP server
	Qup(
		qup_embassy::ServerError<
			edge_nal::io::ReadExactError<embassy_net::tcp::Error>,
			embassy_net::tcp::Error,
		>,
	),
	/// An error occurred with the mDNS resolver
	Mdns(edge_mdns::io::MdnsIoError<edge_nal_embassy::UdpError>),
	/// The network device went down before we could wait for mDNS discovery.
	LinkDown,
	/// The mDNS responder stopped unexpectedly while waiting for a connection.
	MdnsStopped,
	/// An error occurred with the TCP listener
	Accept(embassy_net::tcp::AcceptError),
	/// The QUP stack stopped unexpectedly after a connection was established.
	QupStopped,
}

pub struct Config {
	pub stack: Stack<'static>,
}

#[embassy_executor::task]
pub async fn run(config: Config) -> ! {
	let Err(err) = run_qup(config.stack).await;
	defmt::error!("QUP initialization failed; resetting in 5s: {:?}", err);
	Timer::after_secs(5).await;
	// SAFETY: QUP failures must reset.
	unsafe {
		crate::reset();
	}
}

/// # Panics
/// Can only be called once. The board should reset if the connection
/// is lost.
async fn run_qup(stack: Stack<'static>) -> Result<!, Error> {
	defmt::debug!("waiting for stack to be configured");
	stack.wait_config_up().await;

	let Some(our_endpoint) = stack.config_v4() else {
		defmt::error!("the link returned None for the ipv4 config");
		return Err(Error::LinkDown);
	};

	static TX_BUFFER: StaticCell<[u8; 4096]> = StaticCell::new();
	static RX_BUFFER: StaticCell<[u8; 4096]> = StaticCell::new();

	let tx_buffer = TX_BUFFER.init([0; 4096]);
	let rx_buffer = RX_BUFFER.init([0; 4096]);

	defmt::debug!("creating listener for QUP connection");
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
		ttl:      edge_mdns::domain::base::Ttl::from_secs(5),
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
		listener.accept(embassy_net::IpListenEndpoint {
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
			defmt::info!("QUP client connected");
		}
		Either::Second(Err(err)) => {
			defmt::error!("TCP listener error: {:?}", err);
			return Err(Error::Accept(err));
		}
	}

	// Force mDNS responder to stop.
	drop(mdns);
	drop(signal);
	drop(mdns_socket);

	let qup_listener = QupSocket(&mut listener);

	crate::vars::run_qup_for_all_vars!(qup_listener).map_err(Error::Qup)?;

	defmt::error!("QUP poller stopped");
	Err(Error::QupStopped)
}

struct QupSocket<'a, 'sock>(&'a mut embassy_net::tcp::TcpSocket<'sock>);

impl qup_core::io::asynch::AsyncByteRead for QupSocket<'_, '_> {
	type Error = edge_nal::io::ReadExactError<embassy_net::tcp::Error>;

	async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), Self::Error> {
		self.0.read_exact(buf).await
	}
}

impl qup_core::io::asynch::AsyncByteWrite for QupSocket<'_, '_> {
	type Error = embassy_net::tcp::Error;

	async fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
		self.0.write_all(buf).await
	}
}
