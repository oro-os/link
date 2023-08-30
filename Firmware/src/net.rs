use crate::uc::DateTime;
use defmt::{debug, error, warn};
use embassy_net::{dns::DnsQueryType, driver::Driver, tcp::TcpSocket, Stack};
use embassy_time::{Duration, Timer};

trait ParseAsciiNum
where
	Self: Sized,
{
	fn parse_ascii_num(v: &[u8]) -> Option<Self>;
}

const _: () = {
	macro_rules! impl_pan {
		($T:ty) => {
			impl ParseAsciiNum for $T {
				fn parse_ascii_num(v: &[u8]) -> Option<Self> {
					if v.is_empty() {
						None
					} else {
						let mut r = Default::default();
						for b in v {
							if *b < b'0' || *b > b'9' {
								return None;
							}
							r = r * 10 + ((*b - b'0') as Self);
						}
						Some(r)
					}
				}
			}
		};
	}
	impl_pan!(u8);
	impl_pan!(u16);
};

pub async fn get_datetime<D: Driver + 'static>(stack: &Stack<D>) -> Option<DateTime> {
	debug!("getting unixtime from worldtimeapi.org");

	// Resolve the worldtimeapi host
	let ip = match stack.dns_query("worldtimeapi.org", DnsQueryType::A).await {
		Ok(ip_vec) if !ip_vec.is_empty() => ip_vec[0],
		Ok(_) => {
			error!("failed to resolve 'worldtimeapi.org': no A records returned");
			return None;
		}
		Err(err) => {
			error!("failed to resolve 'worldtimeapi.org': {:?}", err);
			return None;
		}
	};

	debug!("worldtimeapi.org resolved: {:?}", ip);

	let mut recv = [0u8; 2048];
	let mut nread;
	loop {
		let mut tx_buf = [0u8; 128];
		let mut rx_buf = [0u8; 2048];
		let mut sock = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);

		if let Err(err) = sock.connect((ip, 80)).await {
			error!("failed to connect to worldtimeapi.org: {:?}", err);
			return None;
		}

		debug!("connected to worldtimeapi.org");

		let res = sock
			.write(
				b"GET /api/ip.txt HTTP/1.1\r\nHost: worldtimeapi.org\r\nConnection: close\r\n\r\n",
			)
			.await;
		if let Err(err) = res {
			error!("failed to send request to worldtimeapi.org: {:?}", err);
			return None;
		}

		debug!("sent request to worldtimeapi.org");

		nread = match sock.read(&mut recv).await {
			Ok(nread) => nread,
			Err(err) => {
				error!("failed to read response from worldtimeapi.org: {:?}", err);
				return None;
			}
		};

		if nread == 0 {
			warn!("got empty response from worldtimeapi.org; retrying...");
			Timer::after(Duration::from_millis(500)).await;
			continue;
		}

		if nread == 0 {
			error!("failed to read response from worldtimeapi.org: empty response");
			return None;
		}

		debug!("read {} bytes from worldtimeapi.org", nread);
		break;
	}

	let response = &recv[0..nread];
	let mut res = DateTime::default();
	let mut relevant_fields = 3;
	for line in response.split(|c| *c == b'\n') {
		let mut iter = line.splitn(2, |c| *c == b':');
		if let (Some(key), Some(val)) = (iter.next(), iter.next()) {
			match key {
				b"datetime" => {
					relevant_fields -= 1;

					let mut found_all = false;

					for (i, v) in val
						.trim_ascii()
						.split(|c| matches!(*c, b'-' | b'T' | b':' | b'.' | b'+'))
						.enumerate()
					{
						match i {
							0 => res.year = u16::parse_ascii_num(v)?,
							1 => res.month = u8::parse_ascii_num(v)?,
							2 => res.day = u8::parse_ascii_num(v)?,
							3 => res.hour = u8::parse_ascii_num(v)?,
							4 => res.minute = u8::parse_ascii_num(v)?,
							5 => res.second = u8::parse_ascii_num(v)?,
							6 | 7 => {}
							8 => {
								found_all = true;
							}
							_ => {
								return None;
							}
						}
					}

					if !found_all {
						return None;
					}
				}
				b"day_of_week" => {
					relevant_fields -= 1;
					res.day_of_week = u8::parse_ascii_num(val.trim_ascii())?;
				}
				b"dst" => {
					relevant_fields -= 1;
					res.dst = val.trim_ascii() == b"true";
				}
				_ => {}
			}
		}
	}

	if relevant_fields == 0 {
		Some(res)
	} else {
		None
	}
}
