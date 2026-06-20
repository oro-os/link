use embassy_net::{
	IpEndpoint, Ipv4Address, Stack,
	dns::QueryResult,
};
use embassy_time::{Duration, Timer};

const MDNS_MULTICAST: Ipv4Address = Ipv4Address::new(224, 0, 0, 251);

pub struct Config {
	pub stack: Stack<'static>,
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, config: Config) -> ! {
	let Config { stack } = config;

	defmt::debug!("waiting for network...");
	stack.wait_config_up().await;
	defmt::debug!("network is up; looking for area controllers...");

	stack
		.join_multicast_group(MDNS_MULTICAST)
		.expect("failed to join multicast group");

	let dns = embassy_net::dns::DnsSocket::new(stack);
	let mut existing = None;
	loop {
		if let Some(service) = resolve_service(&dns).await {
			defmt::trace!("found mDNS service: {:?}", service);
			if let Some(existing) = &existing {
				if *existing != service {
					defmt::warn!("service record changed: {:?} -> {:?}", existing, service);
					defmt::warn!("rebooting");
					// SAFETY: Rebooting is the best way to recover; we've been asked to connect
					// SAFETY: to a different area controller.
					unsafe {
						crate::reset();
					}
				}
			} else {
				defmt::info!("found service record: {:?}", service);
				existing = Some(service);
				bus.svc_redis.send(super::svc_redis::Cmd::Connect { endpoint: service }).await;
			}
		} else {
			defmt::warn!("no area controller found via mDNS; checking again in 3s");
			Timer::after(Duration::from_secs(3)).await;
			continue;
		};

		Timer::after(Duration::from_secs(10)).await;
	}
}

async fn resolve_service(dns: &embassy_net::dns::DnsSocket<'_>) -> Option<IpEndpoint> {
	let result = match dns.query("_oro-link-aread._tcp.local", embassy_net::dns::DnsQueryType::Ptr).await {
		Ok(r) => r,
		Err(e) => {
			defmt::warn!("mDNS PTR query failed: {:?}", e);
			return None;
		}
	};

	for record in result {
		if let QueryResult::Ptr(rec) = record {
			defmt::debug!("found PTR record: {}", defmt::Display2Format(&rec));
			let srv_result = match dns.query(rec, embassy_net::dns::DnsQueryType::Srv).await {
				Ok(r) => r,
				Err(e) => {
					defmt::warn!("mDNS SRV query failed: {:?}", e);
					continue;
				}
			};

			for srv_record in srv_result {
				if let QueryResult::Srv(srv) = srv_record {
					defmt::debug!(
						"found SRV record: priority {}, weight {}, port {}, target {}",
						srv.priority,
						srv.weight,
						srv.port,
						defmt::Display2Format(&srv.target)
					);
					let port = srv.port;

					let a_result = match dns.query(&srv.target, embassy_net::dns::DnsQueryType::A).await {
						Ok(r) => r,
						Err(e) => {
							defmt::warn!("mDNS A query failed: {:?}", e);
							continue;
						}
					};

					for a_record in a_result {
						if let QueryResult::Address(a) = a_record {
							defmt::debug!("found A record: {}", a);
							return Some(IpEndpoint::new(a, port));
						}
					}
				}
			}
		}
	}

	None
}
