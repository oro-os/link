use defmt::{debug, error};
use embassy_net::{dns::DnsQueryType, driver::Driver, tcp::TcpSocket, Stack};

pub async fn get_unixtime<D: Driver + 'static>(stack: &Stack<D>) -> Option<u64> {
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

	let mut tx_buf = [0u8; 256];
	let mut rx_buf = [0u8; 512];
	let mut sock = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);

	if let Err(err) = sock.connect((ip, 80)).await {
		error!("failed to connect to worldtimeapi.org: {:?}", err);
		return None;
	}

	debug!("connected to worldtimeapi.org");

	let res = sock
		.write(b"GET /api/ip.txt HTTP/1.1\r\nHost: worldtimeapi.org\r\nConnection: close\r\n\r\n")
		.await;
	if let Err(err) = res {
		error!("failed to send request to worldtimeapi.org: {:?}", err);
		return None;
	}

	debug!("sent request to worldtimeapi.org");

	let mut recv = [0u8; 512];
	let nread = match sock.read(&mut recv).await {
		Ok(nread) => nread,
		Err(err) => {
			error!("failed to read response from worldtimeapi.org: {:?}", err);
			return None;
		}
	};

	let recvbuf = &recv[..nread];
	if recvbuf.is_empty() {
		error!("failed to read response from worltimeapi.org: empty response");
		return None;
	}

	debug!("read {} bytes from worldtimeapi.org", recvbuf.len());

	None
}
