use crate::command::{Command, CommandReceiver};
use defmt::{debug, trace, warn};
use embassy_futures::select::{select, Either};
use embassy_net::{
	driver::Driver,
	udp::{PacketMetadata, UdpSocket},
	Stack,
};
use link_protocol::Packet;
use smoltcp::wire::{
	DhcpMessageType, DhcpOption, DhcpPacket, DhcpRepr, EthernetAddress, IpAddress, IpEndpoint,
	Ipv4Address, DHCP_CLIENT_PORT, DHCP_SERVER_PORT,
};

const TFTP_SERVER: &str = "10.0.0.1";
const TFTP_BOOTFILE_BIOS: &str = "ORO_BOOT_BIOS";
const TFTP_BOOTFILE_UEFI: &str = "ORO_BOOT_UEFI";

const BASE_RESPONSE: DhcpRepr = DhcpRepr {
	message_type: DhcpMessageType::Offer,
	transaction_id: 0,
	secs: 0,
	client_hardware_address: EthernetAddress([0, 0, 0, 0, 0, 0]),
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
	dns_servers: None,
	max_size: None,
	lease_duration: Some(u32::MAX),
	renew_duration: None,
	rebind_duration: None,
	additional_options: &[],
};

pub async fn run<D: Driver + 'static, const R: usize>(
	stack: &Stack<D>,
	receiver: CommandReceiver<R>,
) -> ! {
	let mut rx_meta = [PacketMetadata::EMPTY; 16];
	let mut tx_meta = [PacketMetadata::EMPTY; 16];
	let mut rx_buffer = [0; 2048];
	let mut tx_buffer = [0; 2048];
	let mut buf = [0; 2048];

	let mut socket = UdpSocket::new(
		stack,
		&mut rx_meta,
		&mut rx_buffer,
		&mut tx_meta,
		&mut tx_buffer,
	);

	socket.bind(DHCP_SERVER_PORT).unwrap();

	// https://www.iana.org/assignments/bootp-dhcp-parameters/bootp-dhcp-parameters.xhtml
	let mut bootfile_size_bios = 0;
	let mut bootfile_size_uefi = 0;
	let mut use_uefi = false;

	loop {
		let (n, _) = match select(socket.recv_from(&mut buf), receiver.receive()).await {
			Either::First(packet) => packet.unwrap(),
			Either::Second(command) => {
				match command {
					Command::IncomingPacket(Packet::BootfileSize { bios, uefi }) => {
						{
							bootfile_size_bios = ((bios + 511) / 512).try_into().unwrap_or_else(|_| {
								warn!("pxe: BIOS bootfile size is too large ({} bytes) to fit into u16 as 512-byte chunks ({} chunks of 512 bytes, which is > 65535)", bios, (bios + 511) / 512);
								u16::MAX
							});

							trace!(
								"pxe: BIOS bootfile size set to {} chunks of 512 bytes",
								bootfile_size_bios
							);
						}

						{
							bootfile_size_uefi = ((uefi + 511) / 512).try_into().unwrap_or_else(|_| {
								warn!("pxe: UEFI bootfile size is too large ({} bytes) to fit into u16 as 512-byte chunks ({} chunks of 512 bytes, which is > 65535)", uefi, (uefi + 511) / 512);
								u16::MAX
							});

							trace!(
								"pxe: UEFI bootfile size set to {} chunks of 512 bytes",
								bootfile_size_uefi
							);
						}
					}
					unknown => {
						warn!("pxe: ignoring unknown command: {:?}", unknown);
					}
				}
				continue;
			}
		};

		let packet = if let Ok(packet) = DhcpPacket::new_checked(&buf[..n]) {
			packet
		} else {
			warn!("pxe: invalid DHCP length; dropping: {}", n);
			continue;
		};

		let request = match DhcpRepr::parse(&packet) {
			Ok(request) => request,
			Err(err) => {
				warn!("pxe: failed to parse DHCP request; dropping: {}", err);
				continue;
			}
		};

		if request.message_type == DhcpMessageType::Discover {
			let mut requested_tftp_server = false;
			let mut requested_boot_file = false;

			// https://www.iana.org/assignments/bootp-dhcp-parameters/bootp-dhcp-parameters.xhtml
			for option in packet.options() {
				debug!("@@@@ {}", option.kind);
				match option.kind {
					// 55 - Parameter Request List
					55 => {
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
					// 93 - Client System Architecture Type
					93 => {
						let arch =
							u16::from_be_bytes(option.data[0..2].try_into().unwrap_or_else(|_| {
								warn!("pxe: peer sent invalid option 93 data");
								[0, 0]
							}));

						match arch {
							// 0 - BIOS
							0 => {
								debug!("pxe: peer requested BIOS boot file");
							}
							// 6 - EFI Byte Code / 7 - UEFI Byte Code
							6 | 7 => {
								debug!("pxe: peer requested UEFI boot file");
								use_uefi = true;
							}
							unknown => {
								warn!(
									"pxe: DHCP offer contained unknown architecture type: {}",
									unknown
								);
							}
						}
					}
					_ => {}
				}
			}

			if !requested_boot_file {
				warn!("pxe: peer didn't request boot file in DHCP request; dropping");
				continue;
			}

			if !requested_tftp_server {
				warn!("pxe: peer didn't request TFTP server in DHCP request; dropping");
				continue;
			}
		}

		let current_bootfile_size_bytes = if use_uefi {
			bootfile_size_uefi.to_be_bytes()
		} else {
			bootfile_size_bios.to_be_bytes()
		};

		let additional_options = [
			// 13 - Boot File Size (in 512 byte chunks)
			DhcpOption {
				kind: 13,
				data: &current_bootfile_size_bytes[..],
			},
			// 66 - TFTP server name
			DhcpOption {
				kind: 66,
				data: TFTP_SERVER.as_bytes(),
			},
			// 67 - bootfile name
			DhcpOption {
				kind: 67,
				data: if use_uefi {
					TFTP_BOOTFILE_UEFI.as_bytes()
				} else {
					TFTP_BOOTFILE_BIOS.as_bytes()
				},
			},
		];

		let response = match request.message_type {
			DhcpMessageType::Discover => {
				trace!("pxe: got DHCP discovery");

				let mut response = BASE_RESPONSE.clone();
				response.secs = request.secs;
				response.client_hardware_address = request.client_hardware_address;
				response.transaction_id = request.transaction_id;
				response.additional_options = &additional_options;

				trace!("pxe: sending offer (len={})", response.buffer_len());

				response
			}
			DhcpMessageType::Request => {
				match request.requested_ip {
					Some(ip) => {
						if ip != Ipv4Address([10, 0, 0, 2]) {
							warn!("pxe: peer requested IP that we didn't offer; dropping");
							continue;
						}
					}
					None => {
						warn!("pxe: peer sent DHCP request without a requested IP; dropping");
						continue;
					}
				}

				trace!("pxe: got DHCP request");

				let mut response = BASE_RESPONSE.clone();
				response.message_type = DhcpMessageType::Ack;
				response.secs = request.secs;
				response.client_hardware_address = request.client_hardware_address;
				response.transaction_id = request.transaction_id;
				response.additional_options = &additional_options;

				trace!("pxe: sending ack (len={})", response.buffer_len());

				response
			}
			mtype => {
				warn!("pxe: invalid DHCP request; dropping: {:?}", mtype);
				continue;
			}
		};

		let mut response_buf = [0; 2048];
		let mut response_packet = DhcpPacket::new_checked(&mut response_buf[..]).unwrap();
		response.emit(&mut response_packet).unwrap();

		match socket
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
				trace!("pxe: sent DHCP response");
			}
			Err(err) => {
				warn!("pxe: failed to send DHCP response: {:?}", err);
			}
		}
	}
}
