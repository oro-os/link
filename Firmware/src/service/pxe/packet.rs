use smoltcp::{
	time::Duration,
	wire::{
		EthernetAddress, EthernetFrame, EthernetProtocol, Icmpv6Message, Icmpv6Packet, IpAddress,
		IpProtocol, Ipv6Address, Ipv6Packet, NdiscRouterFlags,
	},
};

pub fn icmpv6_router_advertisement(buf: &mut [u8], src_addr: EthernetAddress) -> usize {
	let mut eth_frame = EthernetFrame::new_checked(buf).unwrap();
	eth_frame.set_src_addr(src_addr);
	// Ipv6 multicast 0x02 address
	eth_frame.set_dst_addr(EthernetAddress([0x33, 0x33, 0x00, 0x00, 0x00, 0x02]));
	eth_frame.set_ethertype(EthernetProtocol::Ipv6);
	let eth_payload_len = eth_frame.payload_mut().len();

	EthernetFrame::<&mut [u8]>::header_len() + {
		// IPv4 mapped 10.0.0.1
		let src_addr = Ipv6Address([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0x0a, 0, 0, 0x01]);
		// All nodes address (ff02::1)
		// https://www.menandmice.com/blog/ipv6-reference-multicast#well-known-ipv6-multicast-addresses
		let dst_addr = Ipv6Address([0xff, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01]);

		let mut ipv6_packet = Ipv6Packet::new_checked(eth_frame.payload_mut()).unwrap();
		ipv6_packet.set_src_addr(src_addr);
		ipv6_packet.set_dst_addr(dst_addr);
		ipv6_packet.set_hop_limit(255);
		ipv6_packet.set_version(6);
		ipv6_packet.set_next_header(IpProtocol::Icmpv6);
		ipv6_packet.set_payload_len((eth_payload_len - ipv6_packet.header_len()) as u16);

		let icmp_len = {
			let mut icmpv6_packet = Icmpv6Packet::new_unchecked(ipv6_packet.payload_mut());

			icmpv6_packet.set_msg_type(Icmpv6Message::RouterAdvert);
			// Managed tells the peer that DHCP is available
			// https://www.arubanetworks.com/techdocs/AOS-CX/10.07/HTML/5200-7864/Content/Chp_IPv6_RA/IPv6_RA_cmds/ipv-nd-ra-man-con-fla-10.htm
			icmpv6_packet.set_router_flags(NdiscRouterFlags::MANAGED);
			icmpv6_packet.set_router_lifetime(Duration::from_secs(10 * 60));

			icmpv6_packet.header_len()
		};

		ipv6_packet.set_payload_len(icmp_len as u16);

		// Must occur after packet is constructed
		{
			let mut icmpv6_packet = Icmpv6Packet::new_unchecked(ipv6_packet.payload_mut());

			icmpv6_packet.fill_checksum(&IpAddress::Ipv6(src_addr), &IpAddress::Ipv6(dst_addr));
		}

		ipv6_packet.total_len()
	}
}
