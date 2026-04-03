use core::net::Ipv4Addr;

use embassy_net::{IpAddress, Stack};
use smoltcp::wire::DnsQueryType;

#[derive(defmt::Format, Debug)]
pub enum Error {
	/// The DNS query failed.
	Failure(embassy_net::dns::Error),
	/// No A record was found in the DNS response.
	NoARecord,
}

pub async fn resolve(stack: Stack<'static>, host: &str) -> Result<Ipv4Addr, Error> {
	// Try to parse as an IP first.
	if let Ok(ip) = host.parse::<Ipv4Addr>() {
		defmt::debug!("parsed host '{}' as IP address {}", host, ip);
		return Ok(ip);
	}

	defmt::debug!("resolving host '{}'", host);
	let addrs = match stack.dns_query(host, DnsQueryType::A).await {
		Ok(addrs) => addrs,
		Err(err) => {
			defmt::warn!("DNS query for host '{}' failed: {:?}", host, err);
			return Err(Error::Failure(err));
		}
	};

	let addr = addrs
		.into_iter()
		.filter_map(|addr| {
			match addr {
				IpAddress::Ipv4(ip) => Some(ip),
				_ => None,
			}
		})
		.next();

	match addr {
		Some(ip) => {
			defmt::debug!("DNS query for host '{}' succeeded: {}", host, ip);
			Ok(ip)
		}
		None => {
			defmt::debug!("DNS query for host '{}' did not contain an A record", host);
			Err(Error::NoARecord)
		}
	}
}
