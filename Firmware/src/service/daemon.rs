use crate::{
	command::{Command, CommandReceiver, CommandSender},
	uc,
};
use aes::{
	cipher::{BlockDecrypt, BlockEncrypt, KeyInit},
	Aes256Dec, Aes256Enc,
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
	Deserialize, Error as LinkPacketError, LinkPacket, Read as LinkProtoRead, Serialize,
	Write as LinkProtoWrite,
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

		info!("daemon: initializing connection to daemon...");
		if connect_to_oro(stack, &mut sock).await.is_err() {
			warn!("daemon: failed to connect to daemon; retrying after 10s...");
			Timer::after(Duration::from_secs(10)).await;
			continue;
		}

		info!("daemon: connected to Oro daemon, key beginning negotiation...");

		let mut private_key = [0u8; 32];
		rng.fill_bytes(&mut private_key);
		let private_key = curve25519::curve25519_sk(private_key);
		let public_key = curve25519::curve25519_pk(private_key);

		if let Err(err) = sock.write_all(&public_key[..]).await {
			error!(
				"daemon: failed to write public key to socket; retrying in 10s: {:?}",
				err
			);
			Timer::after(Duration::from_secs(10)).await;
			continue;
		}

		let mut their_public_key = [0u8; 32];
		if let Err(err) = sock.read_exact(&mut their_public_key).await {
			error!(
				"daemon: failed to read remote public key from socket; retrying in 10s: {:?}",
				err
			);
			Timer::after(Duration::from_secs(10)).await;
			continue;
		}

		let key = curve25519::curve25519(private_key, their_public_key);

		let enc = Aes256Enc::new_from_slice(&key[..]).unwrap();
		let dec = Aes256Dec::new_from_slice(&key[..]).unwrap();

		debug!("daemon: encryption key negotiated");

		{
			let (receiver, sender) = sock.split();
			let mut incoming = MessageReceiver::new(receiver, dec, broker_sender);
			let mut outgoing = MessageSender::new(sender, enc, daemon_receiver);

			match select(incoming.run(), outgoing.run()).await {
				Either::First(()) => warn!(
					"daemon: incoming channel has closed or errored; terminating daemon connection"
				),
				Either::Second(()) => warn!(
					"daemon: incoming channel has closed or errored; terminating daemon connection"
				),
			}
		}

		sock.abort();
	}
}

struct MessageReceiver<'a, const SZ: usize> {
	sock: TcpReader<'a>,
	tls: Aes256Dec,
	channel: CommandSender<SZ>,
	cursor: usize,
	block: [u8; 16],
}

impl<'a, const SZ: usize> MessageReceiver<'a, SZ> {
	fn new(sock: TcpReader<'a>, tls: Aes256Dec, channel: CommandSender<SZ>) -> Self {
		let s = Self {
			sock,
			tls,
			channel,
			cursor: 16,
			block: [0; 16],
		};

		debug_assert!(s.cursor >= s.block.len());
		s
	}

	async fn receive_packet(&mut self) -> Result<LinkPacket, LinkPacketError> {
		let msg = LinkPacket::deserialize(self).await?;
		self.cursor = self.block.len(); // force a fresh read for the next packet.
		Ok(msg)
	}

	async fn run(&mut self) {
		loop {
			match self.receive_packet().await {
				Ok(msg) => match msg {
					LinkPacket::ResetLink => self.channel.send(Command::Reset).await,
					unknown => {
						warn!(
							"daemon: received unexpected packet from daemon: {:?}",
							unknown
						);
					}
				},
				Err(err) => {
					error!("daemon: received error when receiving packet: {:?}", err);
					return;
				}
			}
		}
	}
}

impl<'a, const SZ: usize> LinkProtoRead for MessageReceiver<'a, SZ> {
	async fn read(&mut self, buf: &mut [u8]) -> Result<(), LinkPacketError> {
		let mut remaining = buf.len();
		let mut off = 0;

		while remaining > 0 {
			trace!(
				"daemon: read(): remaining={} off={} cursor={}",
				remaining,
				off,
				self.cursor
			);

			if self.cursor >= self.block.len() {
				debug_assert_eq!(self.cursor, self.block.len());

				trace!("daemon: read(): reading 16 bytes from stream");

				self.sock
					.read_exact(&mut self.block[..])
					.await
					.map_err(|err| {
						error!("daemon: failed reading block from socket: {:?}", err);
						LinkPacketError::Eof
					})?;

				self.cursor = 0;

				trace!("daemon: decrypting bytes from stream");

				self.tls.decrypt_block((&mut self.block[..]).into());
			}

			let to_write = remaining.min(self.block.len() - self.cursor);
			trace!("daemon: to write: {}", to_write);
			buf[off..off + to_write]
				.copy_from_slice(&self.block[self.cursor..self.cursor + to_write]);
			remaining -= to_write;
			off += to_write;
			self.cursor += to_write;
		}

		Ok(())
	}
}

struct MessageSender<'a, const SZ: usize> {
	sock: TcpWriter<'a>,
	tls: Aes256Enc,
	channel: CommandReceiver<SZ>,
	block: [u8; 16],
	cursor: usize,
}

impl<'a, const SZ: usize> MessageSender<'a, SZ> {
	fn new(sock: TcpWriter<'a>, tls: Aes256Enc, channel: CommandReceiver<SZ>) -> Self {
		Self {
			sock,
			tls,
			channel,
			block: [0; 16],
			cursor: 0,
		}
	}

	async fn send_packet(&mut self, packet: LinkPacket) -> Result<(), LinkPacketError> {
		packet.serialize(self).await?;

		// Flush any remaining contents in the last block
		// Kind of a hack to DRY up the sending sequence...
		if self.cursor > 0 {
			for _ in self.cursor..self.block.len() {
				<Self as LinkProtoWrite>::write(self, &[0]).await?;
			}
		}

		Ok(())
	}

	async fn run(&mut self) {
		loop {
			let result = match self.channel.receive().await {
				Command::DaemonHello { uid, version } => {
					self.send_packet(LinkPacket::LinkOnline { uid, version })
						.await
				}
				unknown => {
					warn!(
						"daemon: unknown command received (by firmware); don't know what to send: {:?}",
						unknown
					);
					continue;
				}
			};

			if let Err(err) = result {
				error!("daemon: failed to send packet: {:?}", err);
				return;
			}
		}
	}
}

impl<'a, const SZ: usize> LinkProtoWrite for MessageSender<'a, SZ> {
	async fn write(&mut self, buf: &[u8]) -> Result<(), LinkPacketError> {
		let mut remaining = buf.len();
		let mut off = 0;

		while remaining > 0 {
			debug_assert!(self.cursor <= self.block.len());

			let to_write = remaining.min(self.block.len() - self.cursor);
			self.block[self.cursor..self.cursor + to_write]
				.copy_from_slice(&buf[off..off + to_write]);
			remaining -= to_write;
			off += to_write;
			self.cursor += to_write;

			if self.cursor >= self.block.len() {
				debug_assert_eq!(self.cursor, self.block.len());

				self.tls.encrypt_block((&mut self.block[..]).into());

				self.sock.write_all(&self.block[..]).await.map_err(|err| {
					error!("daemon: failed to write block: {:?}", err);
					LinkPacketError::Eof
				})?;

				self.cursor = 0;
			}
		}

		Ok(())
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
