use std::collections::HashMap;

use anyhow::Result;

use crate::link_id::LinkId;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
	#[serde(default)]
	pub link:     HashMap<String, LinkConfig>,
	#[serde(flatten)]
	pub reserved: HashMap<String, toml::Value>,
}

impl Config {
	pub fn normalize(&mut self) -> Result<()> {
		let mut normalized_links = HashMap::new();
		for (name, config) in self.link.drain() {
			let normalized_name = name.trim().to_ascii_uppercase();
			let link_id = LinkId::try_from(normalized_name.as_str())
				.map_err(|_| anyhow::anyhow!("invalid link id '{}'", name.trim()))?;
			let canonical_name = link_id.to_string();

			if normalized_links.contains_key(&canonical_name) {
				return Err(anyhow::anyhow!(
					"duplicate link name after normalization: '{}'",
					canonical_name
				));
			}

			log::debug!("normalized link name '{}' to '{}'", name, canonical_name);
			normalized_links.insert(canonical_name, config);
		}
		self.link = normalized_links;
		Ok(())
	}
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct LinkConfig {
	pub power_type: PowerType,
	pub usb_iface: UsbIface,
	pub boot_source: BootSource,
	pub require_4a_vbus: bool,
	pub wol: Wol,
	#[serde(flatten)]
	pub reserved: HashMap<String, toml::Value>,
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
	#[serde(rename = "usb_vbus", alias = "vbus")]
	UsbVbus,
	#[serde(rename = "psu")]
	Psu,
}

impl PowerType {
	pub fn as_qup_value(self) -> String {
		serialize_plain_value(self)
	}
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub enum UsbIface {
	#[serde(rename = "port")]
	Port,
	#[serde(rename = "header")]
	Header,
}

impl UsbIface {
	pub fn as_qup_value(self) -> String {
		serialize_plain_value(self)
	}
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub enum BootSource {
	#[serde(rename = "usb", alias = "usb_msd")]
	Usb,
	#[serde(rename = "sd")]
	Sd,
}

impl BootSource {
	pub fn as_qup_value(self) -> String {
		serialize_plain_value(self)
	}
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub enum Wol {
	#[serde(rename = "never", alias = "off")]
	Never,
	#[serde(rename = "5m")]
	Mins5,
	#[serde(rename = "10m")]
	Mins10,
	#[serde(rename = "30m")]
	Mins30,
}

impl Wol {
	pub fn as_qup_value(self) -> String {
		serialize_plain_value(self)
	}
}
