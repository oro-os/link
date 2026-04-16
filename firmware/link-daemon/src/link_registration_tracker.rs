use std::{
	collections::{HashMap, HashSet},
	time::Duration,
};

use anyhow::{Context, Result};

use crate::link_id::LinkId;

pub async fn run(mqtt_host: String, mqtt_port: u16) -> Result<!> {
	let mut client = mqtt_async_client::client::Client::builder()
		.set_url_string(&format!("mqtt://{mqtt_host}:{mqtt_port}"))
		.context("failed to set MQTT URL string")?
		.set_keep_alive(mqtt_async_client::client::KeepAlive::Enabled { secs: 5 })
		.set_client_id(Some("link-daemon".into()))
		.set_automatic_connect(true)
		.set_connect_retry_delay(Duration::from_secs(3))
		.build()
		.context("failed to build MQTT client")?;

	log::info!("connecting to MQTT");
	client
		.connect()
		.await
		.context("failed to connect to MQTT")?;

	// Subscribe to the registration topic
	client
		.subscribe(mqtt_async_client::client::Subscribe::new(vec![
			mqtt_async_client::client::SubscribeTopic {
				topic_path: "orolink/register".into(),
				qos:        mqtt_async_client::client::QoS::AtLeastOnce,
			},
		]))
		.await
		.context("failed to subscribe to orolink/register")?;

	let mut active_registrations = HashMap::<LinkId, tokio::task::JoinHandle<!>>::new();

	loop {
		let notif = client
			.read_subscriptions()
			.await
			.context("failed to receive orolink/register notification")?;
		if notif.topic() != "orolink/register" {
			log::warn!(
				"received out-of-band notification to orolink/register subscription: {}",
				notif.topic()
			);
			continue;
		}

		let Ok(registrations) = std::str::from_utf8(notif.payload()) else {
			log::warn!("received invalid utf-8 string from orolink/register");
			continue;
		};

		log::debug!("got registrations string: {registrations}");

		for registration in registrations.split(';') {
			log::debug!("processing individual registration: {registration}");
			let registration = registration.trim();
			if registration.is_empty() {
				log::warn!("received empty registration from orolink/register");
				continue;
			}

			match registration.as_bytes()[0] {
				b'-' => {
					let Ok(id) = LinkId::try_from(&registration[1..]) else {
						log::warn!(
							"invalid de-registration link ID from orolink/register: {registration}"
						);
						continue;
					};

					if let Some(runner) = active_registrations.remove(&id) {
						log::info!("de-registering runner for link {id}");
						runner.abort_handle().abort();
					} else {
						log::debug!("skipping inactive runner de-registration: {id}");
					}
				}
				b'+' => {
					let Some((registration, machine_labels)) = (registration[1..]).split_once('=')
					else {
						log::warn!(
							"registration to orolink/register missing `=machine,labels`: \
							 {registration}"
						);
						continue;
					};
					let Ok(id) = LinkId::try_from(registration) else {
						log::warn!(
							"invalid registration link ID from orolink/register: {registration}"
						);
						continue;
					};

					log::info!("registering new link runner with id: {id}");
					let runner_task = tokio::spawn(run_runner(
						id,
						HashSet::from_iter(machine_labels.split(',').map(Into::into)),
					));
					if let Some(old_task) = active_registrations.insert(id, runner_task) {
						log::warn!("new active registration resulted in an eviction: {id}");
						old_task.abort_handle().abort();
					}
				}
				c => {
					log::warn!("registration string starts with invalid byte: {c:02X}");
					continue;
				}
			}
		}
	}
}

async fn run_runner(id: LinkId, labels: HashSet<String>) -> ! {
	log::info!("started runner for link: {id} (labels: {labels:?})");

	loop {
		tokio::time::sleep(Duration::from_secs(60)).await;
	}
}
