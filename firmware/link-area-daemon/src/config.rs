use std::collections::HashMap;

use anyhow::Result;

/// The configuration format for the area daemon, stored in a file.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
	/// The instance information.
	pub instance: InstanceConfig,
	/// The devices to manage and their configuration.
	#[serde(default)]
	pub link:     HashMap<String, LinkConfig>,
}

impl Config {
	/// Normalizes the link names to uppercase,
	/// since mDNS service names are case-insensitive
	/// and we want to avoid mismatches.
	pub fn normalize(&mut self) -> Result<()> {
		let mut normalized_links = HashMap::new();
		for (name, config) in self.link.drain() {
			let normalized_name = name.trim().to_ascii_uppercase();
			if normalized_links.contains_key(&normalized_name) {
				return Err(anyhow::anyhow!(
					"duplicate link name after normalization: '{}'",
					normalized_name
				));
			}
			log::debug!("normalized link name '{}' to '{}'", name, normalized_name);
			normalized_links.insert(normalized_name, config);
		}
		self.link = normalized_links;
		Ok(())
	}
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct InstanceConfig {
	/// The port to listen on for incoming connections.
	///
	/// Note that the Links are connected *to* via mDNS,
	/// so this port is only used for incoming connections for
	/// explorers, debugging tools, etc.
	///
	/// If `None`, no TCP listener will be started; the service
	/// will only connect to discovered Links, but will not accept
	/// incoming connections (e.g. for debugging).
	pub port:       Option<u16>,
	/// The network interface to bind to (defaults to all interfaces).
	#[serde(default = "default_bind")]
	pub bind:       String,
	/// The IP address to advertise for this instance.
	pub ip_address: String,
	/// The path to the RocksDB database to use for storing
	/// paired link information and retained MQTT data.
	#[serde(default = "default_db_path")]
	pub db_path:    String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct LinkConfig {
	/// The one time password for the link, used for authentication
	/// during initial pairing. After pairing, this is no longer used and can be discarded.
	///
	/// Shown on the OLED on first boot/reset.
	pub otp: String,
}

pub fn default_bind() -> String {
	"0.0.0.0".to_string()
}

pub fn default_db_path() -> String {
	"/var/lib/oro/link-area.db".to_string()
}
