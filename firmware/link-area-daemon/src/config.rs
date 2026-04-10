use std::collections::HashMap;

use anyhow::Result;

/// The configuration format for the area daemon, stored in a file.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
	/// The instance information.
	pub instance: InstanceConfig,
	/// The remote daemon to proxy MQTT traffic to.
	pub daemon:   DaemonConfig,
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
	/// Reserved for future area-controller instance options.
	#[serde(flatten)]
	pub reserved: HashMap<String, toml::Value>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct DaemonConfig {
	/// Hostname of the centralized Oro Link daemon.
	#[serde(default = "default_daemon_host")]
	pub host:        String,
	/// TLS port that carries proxied MQTT traffic.
	#[serde(default = "default_daemon_port")]
	pub port:        u16,
	/// PEM public key or certificate used to pin the daemon's TLS identity.
	pub server_key:  String,
	/// Client certificate used for mutual TLS to the daemon.
	pub client_cert: String,
	/// Client private key matching `client_cert`.
	pub client_key:  String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct LinkConfig {
	/// The one time password for the link, used for authentication
	/// during initial pairing. After pairing, this is no longer used and can be discarded.
	///
	/// Shown on the OLED on first boot/reset.
	pub otp: String,
}

pub fn default_daemon_host() -> String {
	"ci.oro.sh".to_string()
}

pub fn default_daemon_port() -> u16 {
	5544
}
