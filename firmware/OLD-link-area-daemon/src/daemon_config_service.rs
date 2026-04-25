use std::{collections::HashMap, future::pending, sync::Arc};

use anyhow::Result;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use mqttrust::{MqttClient, Publish, QoS, Subscribe, SubscribeTopic};
use tokio::sync::Semaphore;

use crate::{config::LinkConfig, daemon_connection::DaemonConnectionConfig};

const MAX_LINK_CONFIG_TOPICS: usize = 16;
const MQTT_TX_BUFFER_SIZE: usize = 2048;
const MQTT_RX_BUFFER_SIZE: usize = 2048;

pub async fn run(
	links: &HashMap<String, LinkConfig>,
	daemon_connection_config: DaemonConnectionConfig,
	daemon_port: u16,
) -> Result<!> {
	let client_id = daemon_connection_config
		.mqtt_client_id()
		.try_into()
		.map_err(|_| anyhow::anyhow!("daemon MQTT client id is too long"))?;
	let last_will_message = links
		.keys()
		.map(|id| format!("-{id}"))
		.intersperse(";".into())
		.collect::<String>();
	let mqtt_config = mqttrust::Config::builder()
		.client_id(client_id)
		.keepalive_interval(embassy_time::Duration::from_secs(20))
		.backoff_algo(mqtt_backoff)
		.last_will(
			mqttrust::LastWill::builder()
				.qos(QoS::AtLeastOnce)
				.topic("orolink/register")
				.data(last_will_message.as_bytes())
				.build(),
		)
		.build();
	let mut state =
		mqttrust::State::<CriticalSectionRawMutex, MQTT_TX_BUFFER_SIZE, MQTT_RX_BUFFER_SIZE>::new();
	let (mut mqtt_stack, mqtt_client) = mqttrust::new(&mut state, mqtt_config);
	let mut transport = daemon_connection_config.transport(daemon_port);

	log::debug!("starting daemon config service");
	tokio::select! {
		_ = async {
			mqtt_stack.run(&mut transport).await;
		} => {
			anyhow::bail!("daemon MQTT stack stopped unexpectedly");
		}
		res = handle_config_requests(links, &mqtt_client, config_ready) => {
			res?;
		}
	}
}

async fn handle_config_requests(
	links: &HashMap<String, LinkConfig>,
	mqtt_client: &MqttClient<'_, CriticalSectionRawMutex>,
	config_ready: Arc<Semaphore>,
) -> Result<!> {
	// Advertise which links we have available and then signal that
	// we're free to start looking for links.
	let register_message = links
		.iter()
		.map(|(id, config)| format!("+{id}={}", config.machine_action_label))
		.intersperse(";".into())
		.collect::<String>();
	mqtt_client
		.publish(
			mqttrust::Publish::builder()
				.qos(QoS::AtLeastOnce)
				.topic_name("orolink/register")
				.payload(register_message.as_bytes())
				.build(),
		)
		.await
		.map_err(|err| anyhow::anyhow!("failed to announce links via orolink/register: {err:?}"))?;
	config_ready.add_permits(1);

	let request_topics = build_request_topics(links)?;

	if request_topics.is_empty() {
		mqtt_client.wait_connected().await;
		log::warn!("no link config topics are configured; keeping daemon config service connected");
		return pending::<Result<!>>().await;
	}

	loop {
		let subscribe_topics = request_topics
			.iter()
			.map(|topic| SubscribeTopic::builder().topic_path(topic.as_str()).build())
			.collect::<Vec<_>>();
		let mut subscription = mqtt_client
			.subscribe::<MAX_LINK_CONFIG_TOPICS>(
				Subscribe::builder()
					.topics(subscribe_topics.as_slice())
					.build(),
			)
			.await
			.map_err(|err| {
				anyhow::anyhow!("failed to subscribe to daemon config topics: {err:?}")
			})?;
		let topic_count = subscribe_topics.len();
		log::info!("subscribed to {topic_count} daemon config topic(s)");

		while let Some(message) = subscription.next_message().await {
			let topic_name = message.topic_name().to_string();
			drop(message);

			let Some(link_id) = topic_name
				.strip_prefix("orolink/")
				.and_then(|topic| topic.strip_suffix("/status/config"))
			else {
				log::warn!("received config request on unexpected topic '{topic_name}'");
				continue;
			};

			let Some(link_config) = links.get(link_id) else {
				log::warn!("received config request for unknown link '{link_id}'");
				continue;
			};

			log::info!("publishing config for link '{link_id}'");
			publish_link_config(mqtt_client, link_id, link_config).await?;
		}

		log::warn!("daemon config subscription ended, resubscribing");
	}
}

fn build_request_topics(links: &HashMap<String, LinkConfig>) -> Result<Vec<String>> {
	if links.len() > MAX_LINK_CONFIG_TOPICS {
		anyhow::bail!(
			"link config service supports at most {} links, but {} are configured",
			MAX_LINK_CONFIG_TOPICS,
			links.len()
		);
	}

	Ok(links
		.keys()
		.map(|link_id| format!("orolink/{link_id}/status/config"))
		.collect())
}

async fn publish_link_config(
	mqtt_client: &MqttClient<'_, CriticalSectionRawMutex>,
	link_id: &str,
	link_config: &LinkConfig,
) -> Result<()> {
	let power_type = link_config.power_type.as_mqtt_value();
	publish_config_value(
		mqtt_client,
		link_id,
		"config/power_type",
		power_type.as_str(),
	)
	.await?;
	let architecture = link_config.architecture.as_mqtt_value();
	publish_config_value(mqtt_client, link_id, "config/arch", architecture.as_str()).await?;
	let usb_iface = link_config.usb_iface.as_mqtt_value();
	publish_config_value(mqtt_client, link_id, "config/usb_iface", usb_iface.as_str()).await?;
	let boot_source = link_config.boot_source.as_mqtt_value();
	publish_config_value(
		mqtt_client,
		link_id,
		"config/boot_source",
		boot_source.as_str(),
	)
	.await?;
	publish_config_value(
		mqtt_client,
		link_id,
		"config/machine_action_label",
		link_config.machine_action_label.as_str(),
	)
	.await?;
	let require_4a_vbus = link_config.require_4a_vbus.to_string();
	publish_config_value(
		mqtt_client,
		link_id,
		"config/require_4a_vbus",
		require_4a_vbus.as_str(),
	)
	.await?;
	let wol = link_config.wol.as_mqtt_value();
	publish_config_value(mqtt_client, link_id, "config/wol", wol.as_str()).await
}

async fn publish_config_value(
	mqtt_client: &MqttClient<'_, CriticalSectionRawMutex>,
	link_id: &str,
	relative_path: &str,
	payload: &str,
) -> Result<()> {
	let topic = format!("orolink/{link_id}/{relative_path}");
	mqtt_client
		.publish(
			Publish::builder()
				.topic_name(topic.as_str())
				.qos(QoS::AtLeastOnce)
				.payload(payload.as_bytes())
				.build(),
		)
		.await
		.map_err(|err| anyhow::anyhow!("failed to publish '{topic}': {err:?}"))
}

fn mqtt_backoff(attempt: u8) -> Option<embassy_time::Duration> {
	let base_backoff_ms = 500u64.saturating_mul(1u64 << u32::from(attempt.min(8)));
	Some(embassy_time::Duration::from_millis(
		base_backoff_ms.min(30_000),
	))
}
