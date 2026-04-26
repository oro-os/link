use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use qup::{Client, KeyInfo, Value};
use tokio::time::sleep;

use crate::{
	area_controller_tcp_listener::{
		AreaControllerListenerConfig, AreaControllerStream, AreaControllerTcpListener,
	},
	config::LinkConfig,
	link_id::LinkId,
};

const REQUIRED_CAPABILITIES: [&str; 5] = ["Pk", "Ii", "Cc", "Ss", "Ww"];
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct Config {
	pub listen_addr: String,
	pub listen_port: u16,
	pub area_controller_listener_config: AreaControllerListenerConfig,
	pub links: Arc<HashMap<String, LinkConfig>>,
}

pub async fn run(config: Config) -> Result<()> {
	let listener = AreaControllerTcpListener::bind(
		config.area_controller_listener_config.clone(),
		(config.listen_addr.as_str(), config.listen_port),
	)
	.await?;
	let local_addr = listener.local_addr()?;

	log::info!("listening for area controllers on {local_addr}");

	loop {
		let (stream, peer) = listener.accept().await?;
		let links = Arc::clone(&config.links);

		log::info!("accepted connection from area controller {peer}");
		tokio::spawn(async move {
			if let Err(err) = handle_connection(peer, stream, links).await {
				log::warn!("link session for {peer} terminated with error: {err:#}");
			}
		});
	}
}

async fn handle_connection(
	peer: std::net::SocketAddr,
	stream: AreaControllerStream,
	links: Arc<HashMap<String, LinkConfig>>,
) -> Result<()> {
	let mut client = Client::new(stream);
	let caps = client
		.caps()
		.await
		.context("failed to fetch link capabilities")?;
	validate_caps(&caps)?;

	let reported_id = client.identify().await.context("failed to identify link")?;
	let normalized_id = normalize_link_id(&reported_id)?;
	let link_config = links
		.get(&normalized_id)
		.cloned()
		.with_context(|| format!("link '{normalized_id}' is not present in daemon config"))?;

	log::info!("validated link '{normalized_id}' from area controller {peer}");

	let key_lookup = build_key_lookup(
		&client
			.list_keys()
			.await
			.context("failed to list link keys")?,
	)?;
	apply_link_config(&mut client, &normalized_id, &link_config, &key_lookup).await?;

	log::info!("configured link '{normalized_id}' successfully");
	keep_session_alive(&mut client, &normalized_id).await
}

fn validate_caps(caps: &str) -> Result<()> {
	for required in REQUIRED_CAPABILITIES {
		if !caps.contains(required) {
			anyhow::bail!("link capabilities '{caps}' are missing required pair '{required}'");
		}
	}

	Ok(())
}

fn normalize_link_id(value: &str) -> Result<String> {
	let normalized = value.trim().to_ascii_uppercase();
	let parsed = LinkId::try_from(normalized.as_str())
		.map_err(|_| anyhow::anyhow!("link reported invalid id '{value}'"))?;
	Ok(parsed.to_string())
}

fn build_key_lookup(keys: &[KeyInfo]) -> Result<HashMap<String, u16>> {
	let mut lookup = HashMap::with_capacity(keys.len());
	for key in keys {
		if lookup.insert(key.name.clone(), key.keyref).is_some() {
			anyhow::bail!("link reported duplicate key name '{}'", key.name);
		}
	}
	Ok(lookup)
}

async fn apply_link_config(
	client: &mut Client<AreaControllerStream>,
	link_id: &str,
	link_config: &LinkConfig,
	key_lookup: &HashMap<String, u16>,
) -> Result<()> {
	write_key(
		client,
		key_lookup,
		"sut_power_type",
		Value::from(link_config.power_type.as_qup_value()),
	)
	.await
	.with_context(|| format!("failed to write power type for link '{link_id}'"))?;
	write_key(
		client,
		key_lookup,
		"wol",
		Value::from(link_config.wol.as_qup_value()),
	)
	.await
	.with_context(|| format!("failed to write wake-on-LAN mode for link '{link_id}'"))?;
	write_key(
		client,
		key_lookup,
		"sut_usb_iface",
		Value::from(link_config.usb_iface.as_qup_value()),
	)
	.await
	.with_context(|| format!("failed to write USB interface for link '{link_id}'"))?;
	write_key(
		client,
		key_lookup,
		"sut_boot_source",
		Value::from(link_config.boot_source.as_qup_value()),
	)
	.await
	.with_context(|| format!("failed to write boot source for link '{link_id}'"))?;
	write_key(
		client,
		key_lookup,
		"sut_require_4a_vbus",
		Value::from(link_config.require_4a_vbus),
	)
	.await
	.with_context(|| format!("failed to write 4A VBUS requirement for link '{link_id}'"))?;
	write_key(client, key_lookup, "configured", Value::from(true))
		.await
		.with_context(|| format!("failed to mark link '{link_id}' configured"))?;

	Ok(())
}

async fn write_key(
	client: &mut Client<AreaControllerStream>,
	key_lookup: &HashMap<String, u16>,
	key_name: &str,
	value: Value,
) -> Result<()> {
	let keyref = key_lookup
		.get(key_name)
		.copied()
		.with_context(|| format!("link does not expose required key '{key_name}'"))?;
	let _written = client
		.write(keyref, &value)
		.await
		.with_context(|| format!("QUP write failed for key '{key_name}'"))?;
	Ok(())
}

async fn keep_session_alive(
	client: &mut Client<AreaControllerStream>,
	link_id: &str,
) -> Result<()> {
	loop {
		sleep(KEEPALIVE_INTERVAL).await;
		client
			.ping()
			.await
			.with_context(|| format!("keepalive ping failed for link '{link_id}'"))?;
	}
}
