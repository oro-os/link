#![expect(
	private_interfaces,
	reason = "MQTT prefixes are very strictly enforced"
)]

use core::net::{Ipv4Addr, Ipv6Addr};

use edge_mdns::domain::base::Ttl;
use edge_nal::UdpSplit;
use embassy_futures::select::Either;
use embassy_net::{IpListenEndpoint, Stack};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, once_lock::OnceLock};
use embassy_time::Timer;
use mqttrust::MqttClient;
use static_cell::StaticCell;

pub struct Config {
	pub stack: Stack<'static>,
	pub mqtt:  &'static OnceLock<Mqtt>,
}

#[embassy_executor::task]
pub async fn run(config: Config) -> ! {
	let Err(err) = run_mqtt(config.stack, config.mqtt).await;
	defmt::error!("MQTT initialization failed; resetting in 5s: {:?}", err);
	Timer::after_secs(5).await;
	// SAFETY: MQTT failures must reset.
	unsafe {
		crate::reset();
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
	/// The MQTT stack stopped unexpectedly after a connection was established.
	MqttStopped,
}

/// # Panics
/// Can only be called once. The board should reset if the connection
/// is lost.
pub async fn run_mqtt<'stack>(
	stack: Stack<'stack>,
	mqtt: &'static OnceLock<Mqtt>,
) -> Result<!, Error> {
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

	// Force mDNS responder to stop.
	drop(mdns);
	drop(signal);
	drop(mdns_socket);

	// Set up MQTT stack.
	let mut mqtt_transport = ConnectedSocketTransport {
		socket:        listener,
		has_connected: false,
	};
	let config = mqttrust::Config::builder()
		.client_id(name.try_into().unwrap())
		.keepalive_interval(embassy_time::Duration::from_secs(50))
		.build();

	static STATE: StaticCell<mqttrust::State<NoopRawMutex, 1024, 1024>> = StaticCell::new();
	let state = STATE.init(mqttrust::State::new());

	let (mut mqtt_stack, mqtt_client) = mqttrust::new(state, config);

	static CLIENT: StaticCell<mqttrust::MqttClient<'static, NoopRawMutex>> = StaticCell::new();
	mqtt.init(Mqtt {
		client: CLIENT.init(mqtt_client),
		prefix: crate::unique_id(),
	})
	.ok();

	mqtt_stack.run(&mut mqtt_transport).await;
	defmt::error!("MQTT stack stopped unexpectedly");
	Err(Error::MqttStopped)
}

/// A transport layer for MQTT using an already connected socket.
///
/// This struct is useful when the socket is already connected and does not require
/// any additional connection logic.
pub struct ConnectedSocketTransport<S> {
	socket:        S,
	has_connected: bool,
}

impl<S: edge_nal::io::Read + edge_nal::io::Write> mqttrust::transport::Transport
	for ConnectedSocketTransport<S>
{
	type Socket = S;

	/// This method is a no-op since the socket is already connected.
	///
	/// # Returns
	///
	/// `Ok(())` always, as no connection logic is required.
	async fn connect(&mut self) -> Result<(), mqttrust::ConnectionError> {
		if self.has_connected {
			defmt::warn!("connect called on ConnectedSocketTransport, but it's already connected");
			Err(mqttrust::ConnectionError::ConnectionRefused)
		} else {
			self.has_connected = true;
			Ok(())
		}
	}

	/// This method is a no-op since the socket is managed externally.
	///
	/// # Returns
	///
	/// `Ok(())` always, as no disconnection logic is required.
	fn disconnect(&mut self) -> Result<(), mqttrust::ConnectionError> {
		Err(mqttrust::ConnectionError::RequestsDone)
	}

	/// Checks if the transport is currently connected.
	///
	/// # Returns
	///
	/// `true` if the transport has only connected once.
	fn is_connected(&self) -> bool {
		self.has_connected
	}

	/// Provides a mutable reference to the socket used by the transport.
	///
	/// # Returns
	///
	/// `Ok(&mut Self::Socket)` always, as the socket is always available.
	fn socket(&mut self) -> Result<&mut Self::Socket, mqttrust::StateError> {
		if !self.has_connected {
			return Err(mqttrust::StateError::InvalidState);
		}

		Ok(&mut self.socket)
	}
}

/// An error produced by the [`Mqtt`] wrapper.
#[derive(defmt::Format)]
pub enum MqttError {
	Mqtt(mqttrust::Error),
	TopicTooLong,
}

/// A wrapper around the MQTT client that allows it to be
/// shared across the application, as well as scoped
/// to this particular device.
#[derive(Clone)]
pub struct Mqtt {
	client: &'static MqttClient<'static, NoopRawMutex>,
	prefix: &'static str,
}

macro_rules! impl_pubs {
	($($(#[$attr:meta])*$name:ident($retain:expr, $qos:expr),)* $(,)?) => {
		$($(#[$attr])*
		pub async fn $name(&self, topic: impl IntoPrefixedTopic, payload: impl AsRef<[u8]>) -> Result<(), MqttError> {
			let topic = topic.into_prefixed_topic(Prefix(self.prefix)).or(Err(MqttError::TopicTooLong))?;
			self.client.publish(mqttrust::Publish::builder().retain($retain).qos($qos).topic_name(topic.0.as_str()).payload(payload.as_ref()).build()).await.map_err(MqttError::Mqtt)?;
			Ok(())
		})*
	}
}

impl Mqtt {
	impl_pubs! {
		/// Publishes a message to the MQTT broker with the given topic and payload, using QoS 0 (at most once).
		publish_0(false, mqttrust::QoS::AtMostOnce),
		/// Publishes a message to the MQTT broker with the given topic and payload, using QoS 1 (at least once).
		publish_1(false, mqttrust::QoS::AtLeastOnce),
		/// Publishes a message to the MQTT broker with the given topic and payload, using QoS 2 (exactly once).
		publish_2(false, mqttrust::QoS::ExactlyOnce),
		/// Publishes and retains a message to the MQTT broker with the given topic and payload, using QoS 0 (at most once).
		retain_0(false, mqttrust::QoS::AtMostOnce),
		/// Publishes and retains a message to the MQTT broker with the given topic and payload, using QoS 1 (at least once).
		retain_1(false, mqttrust::QoS::AtLeastOnce),
		/// Publishes and retains a message to the MQTT broker with the given topic and payload, using QoS 2 (exactly once).
		retain_2(false, mqttrust::QoS::ExactlyOnce),
	}

	/// Bakes a topic with a given suffix. Returns `Err(topic)` if the topic is too long.
	pub fn try_prepare_topic<T: IntoPrefixedTopic>(&self, topic: T) -> Result<PrefixedTopic, T> {
		topic.into_prefixed_topic(Prefix(self.prefix))
	}

	/// Bakes a topic with the given suffix.
	///
	/// # Panics
	/// Panics if the topic is too long. Use [`Mqtt::try_prepare_topic()`] if panicking is undesireable.
	pub fn prepare_topic<T: IntoPrefixedTopic>(&self, topic: T) -> PrefixedTopic {
		match self.try_prepare_topic(topic) {
			Ok(t) => t,
			Err(_) => {
				panic!("failed to prepare topic; too long");
			}
		}
	}
}

/// Inner, hidden type to prevent `IntoPrefixedTopic` from being misused.
#[repr(transparent)]
struct Prefix(&'static str);

/// Creates a checked [`PrefixedTopic`] from the implementing type.
pub trait IntoPrefixedTopic: Sized {
	/// Returns `Err(self)` if the prefixed topic would be too long.
	fn into_prefixed_topic(self, prefix: Prefix) -> Result<PrefixedTopic, Self>;
}

/// A checked, fully qualified topic that is prefixed with the [`Mqtt`] prefix.
#[derive(defmt::Format)]
pub struct PrefixedTopic(heapless::String<64>);

impl IntoPrefixedTopic for &str {
	fn into_prefixed_topic(self, prefix: Prefix) -> Result<PrefixedTopic, Self> {
		let mut r = heapless::String::new();
		let Ok(_) = r.push_str(prefix.0) else {
			return Err(self);
		};
		let Ok(_) = r.push('/') else {
			return Err(self);
		};
		if (r.len() + self.len()) > r.capacity() {
			return Err(self);
		}
		r.push_str(self).unwrap();
		Ok(PrefixedTopic(r))
	}
}

impl IntoPrefixedTopic for PrefixedTopic {
	#[inline]
	fn into_prefixed_topic(self, _prefix: Prefix) -> Result<PrefixedTopic, Self> {
		Ok(self)
	}
}

impl IntoPrefixedTopic for &PrefixedTopic {
	#[inline]
	fn into_prefixed_topic(self, _prefix: Prefix) -> Result<PrefixedTopic, Self> {
		Ok(PrefixedTopic(self.0.clone()))
	}
}
