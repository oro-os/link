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
	/// The kind of power source that should be used for the link.
	pub power_type: PowerType,
	/// The system-under-test architecture wired to this link.
	pub architecture: SutArch,
	/// How the USB interface should be exposed.
	pub usb_iface: UsbIface,
	/// Which boot source should be selected.
	pub boot_source: BootSource,
	/// Human-readable label for the primary machine action.
	pub machine_action_label: String,
	/// Whether the link requires a 4A-capable VBUS supply.
	pub require_4a_vbus: bool,
	/// Wake-on-LAN retry behavior.
	pub wol: Wol,
}

fn serialize_plain_value<T>(value: T) -> String
where
	T: serde::Serialize,
{
	serde_plain::to_string(&value).expect("config enums should serialize as plain strings")
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub enum PowerType {
	#[serde(rename = "usb")]
	Usb,
	#[serde(rename = "vbus")]
	UsbVbus,
	#[serde(rename = "psu")]
	Psu,
}

impl PowerType {
	pub fn as_mqtt_value(self) -> String {
		serialize_plain_value(self)
	}
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub enum SutArch {
	#[serde(rename = "x86_64-amd")]
	X8664Amd,
	#[serde(rename = "x86_64-intel")]
	X8664Intel,
	#[serde(rename = "aarch64")]
	Aarch64,
	#[serde(rename = "aarch64-mobile")]
	Aarch64Mobile,
	#[serde(rename = "riscv64")]
	Riscv64,
}

impl SutArch {
	pub fn as_mqtt_value(self) -> String {
		serialize_plain_value(self)
	}
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub enum UsbIface {
	#[serde(rename = "port")]
	Port,
	#[serde(rename = "header")]
	Header,
	#[serde(rename = "off")]
	Off,
}

impl UsbIface {
	pub fn as_mqtt_value(self) -> String {
		serialize_plain_value(self)
	}
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub enum BootSource {
	#[serde(rename = "usb_msd")]
	UsbMsd,
	#[serde(rename = "sd")]
	Sd,
}

impl BootSource {
	pub fn as_mqtt_value(self) -> String {
		serialize_plain_value(self)
	}
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub enum Wol {
	#[serde(rename = "off")]
	Off,
	#[serde(rename = "5m")]
	Mins5,
	#[serde(rename = "10m")]
	Mins10,
	#[serde(rename = "30m")]
	Mins30,
}

impl Wol {
	pub fn as_mqtt_value(self) -> String {
		serialize_plain_value(self)
	}
}

pub fn default_daemon_host() -> String {
	"ci.oro.sh".to_string()
}

pub fn default_daemon_port() -> u16 {
	5544
}
