use rmqtt::types::HashMap;

/// The configuration format for the area daemon, stored in a file.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
	/// The instance information.
	pub instance: InstanceConfig,
	/// The devices to manage and their configuration.
	#[serde(default)]
	pub link:     HashMap<String, LinkConfig>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct InstanceConfig {
	/// The port to listen on for incoming connections.
	#[serde(default = "default_port")]
	pub port:       u16,
	/// The network interface to bind to (defaults to all interfaces).
	#[serde(default = "default_bind")]
	pub bind:       String,
	/// The IP addresse to advertise for this instance.
	pub ip_address: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct LinkConfig {
	/// The one time password for the link, used for authentication
	/// during initial pairing. After pairing, this is no longer used and can be discarded.
	///
	/// Shown on the OLED on first boot/reset.
	pub otp: String,
}

pub const fn default_port() -> u16 {
	5544
}

pub fn default_bind() -> String {
	"0.0.0.0".to_string()
}
