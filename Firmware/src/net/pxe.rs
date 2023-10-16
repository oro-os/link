use defmt::{debug, warn};
use embassy_net::{
	driver::Driver,
	udp::{PacketMetadata, UdpSocket},
	Stack,
};
use embassy_time::{Duration, Timer};
use heapless::Vec;
use smoltcp::wire::{
	DhcpMessageType, DhcpOption, DhcpPacket, DhcpRepr, IpAddress, IpEndpoint, Ipv4Address,
	DHCP_CLIENT_PORT, DHCP_SERVER_PORT,
};

pub async fn handshake_dhcp<D: Driver + 'static>(stack: &Stack<D>) {
	static mut DHCP_RX_META: [PacketMetadata; 16] = [PacketMetadata::EMPTY; 16];
	static mut DHCP_TX_META: [PacketMetadata; 16] = [PacketMetadata::EMPTY; 16];
	static mut DHCP_RX_BUFFER: [u8; 2048] = [0; 2048];
	static mut DHCP_TX_BUFFER: [u8; 2048] = [0; 2048];

	let mut buf = [0; 2048];

	let mut dhcp_socket = unsafe {
		UdpSocket::new(
			stack,
			&mut DHCP_RX_META,
			&mut DHCP_RX_BUFFER,
			&mut DHCP_TX_META,
			&mut DHCP_TX_BUFFER,
		)
	};

	dhcp_socket.bind(DHCP_SERVER_PORT).unwrap();

	let (n, _) = dhcp_socket.recv_from(&mut buf).await.unwrap();
	let packet = if let Ok(packet) = DhcpPacket::new_checked(&buf[..n]) {
		packet
	} else {
		warn!("pxe: invalid DHCP length: {}", n);
		return;
	};

	let request = match DhcpRepr::parse(&packet) {
		Ok(request) => request,
		Err(err) => {
			warn!("pxe: failed to parse DHCP request: {}", err);
			return;
		}
	};

	if request.message_type != DhcpMessageType::Discover {
		warn!(
			"pxe: peer sent invalid DHCP message type (expected Discover): {}",
			request.message_type
		);
		return;
	}

	let mut requested_tftp_server = false;
	let mut requested_boot_file = false;

	// https://www.iana.org/assignments/bootp-dhcp-parameters/bootp-dhcp-parameters.xhtml
	for option in packet.options() {
		// 55 - Parameter Request List
		if option.kind == 55 {
			for kind in option.data {
				match kind {
					// 66 - TFTP server name
					66 => {
						requested_tftp_server = true;
					}
					// 67 - bootfile name
					67 => {
						requested_boot_file = true;
					}
					_ => {}
				}
			}
		}
	}

	if !requested_boot_file {
		warn!("pxe: peer didn't request boot file in DHCP request");
		return;
	}

	if !requested_tftp_server {
		warn!("pxe: peer didn't request TFTP server in DHCP request");
		return;
	}

	debug!("pxe: got DHCP discovery");

	const TFTP_SERVER: &str = "10.0.0.1";
	const TFTP_BOOTFILE: &str = "ORO_BOOT";

	let mut response = DhcpRepr {
		message_type: DhcpMessageType::Offer,
		transaction_id: request.transaction_id,
		secs: request.secs,
		client_hardware_address: request.client_hardware_address,
		client_ip: Ipv4Address([0, 0, 0, 0]),
		your_ip: Ipv4Address([10, 0, 0, 2]),
		server_ip: Ipv4Address([10, 0, 0, 1]),
		router: Some(Ipv4Address([10, 0, 0, 1])),
		subnet_mask: Some(Ipv4Address([255, 255, 255, 0])),
		relay_agent_ip: Ipv4Address([0, 0, 0, 0]),
		broadcast: true,
		requested_ip: None,
		client_identifier: None,
		server_identifier: Some(Ipv4Address([10, 0, 0, 1])),
		parameter_request_list: None,
		dns_servers: Some(Vec::from_slice(&[Ipv4Address([10, 0, 0, 1])]).unwrap()),
		max_size: None,
		lease_duration: Some(u32::MAX),
		renew_duration: None,
		rebind_duration: None,
		// https://www.iana.org/assignments/bootp-dhcp-parameters/bootp-dhcp-parameters.xhtml
		additional_options: &[
			// 66 - TFTP server name
			DhcpOption {
				kind: 66,
				data: TFTP_SERVER.as_bytes(),
			},
			// 67 - bootfile name
			DhcpOption {
				kind: 67,
				data: TFTP_BOOTFILE.as_bytes(),
			},
		],
	};

	{
		let mut response_buf = [0; 2048];
		let mut response_packet = DhcpPacket::new_checked(&mut response_buf[..]).unwrap();
		response.emit(&mut response_packet).unwrap();

		debug!("pxe: sending offer (len={})", response.buffer_len());

		match dhcp_socket
			.send_to(
				&response_buf[..response.buffer_len()],
				IpEndpoint {
					addr: IpAddress::Ipv4(Ipv4Address([255, 255, 255, 255])),
					port: DHCP_CLIENT_PORT,
				},
			)
			.await
		{
			Ok(()) => {
				debug!("pxe: sent DHCP offer");
			}
			Err(err) => {
				warn!("pxe: failed to send DHCP offer: {:?}", err);
				return;
			}
		}
	}

	let (n, _) = dhcp_socket.recv_from(&mut buf).await.unwrap();
	let packet = if let Ok(packet) = DhcpPacket::new_checked(&buf[..n]) {
		packet
	} else {
		warn!("pxe: invalid DHCP length: {}", n);
		return;
	};

	let request = match DhcpRepr::parse(&packet) {
		Ok(request) => request,
		Err(err) => {
			warn!("pxe: failed to parse DHCP request: {}", err);
			return;
		}
	};

	if request.message_type != DhcpMessageType::Request {
		warn!(
			"pxe: peer sent invalid DHCP message type (expected Request): {}",
			request.message_type
		);
		return;
	}

	match request.requested_ip {
		Some(ip) => {
			if ip != Ipv4Address([10, 0, 0, 2]) {
				warn!("pxe: peer requested IP that we didn't offer");
				return;
			}
		}
		None => {
			warn!("pxe: peer sent DHCP request without a requested IP");
			return;
		}
	}

	debug!("pxe: got DHCP request");

	response.message_type = DhcpMessageType::Ack;

	{
		let mut response_buf = [0; 2048];
		let mut response_packet = DhcpPacket::new_checked(&mut response_buf[..]).unwrap();
		response.emit(&mut response_packet).unwrap();

		debug!("pxe: sending ack (len={})", response.buffer_len());

		match dhcp_socket
			.send_to(
				&response_buf[..response.buffer_len()],
				IpEndpoint {
					addr: IpAddress::Ipv4(Ipv4Address([255, 255, 255, 255])),
					port: DHCP_CLIENT_PORT,
				},
			)
			.await
		{
			Ok(()) => {
				debug!("pxe: sent DHCP ack");
			}
			Err(err) => {
				warn!("pxe: failed to send DHCP ack: {:?}", err);
				return;
			}
		}
	}

	Timer::after(Duration::from_millis(5000)).await;
}
