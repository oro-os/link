//! This is a hack-and-slash Docker client.
//!
//! Do not use it for anything serious (i.e. copying it from this repo).
//!
//! The current state of HTTP and async `serde` paired with the monoculture
//! of `tokio` usage means that some really nasty stuff had to happen here to
//! complete the Daemon sometime this year. Otherwise I'd have to re-create
//! just about every piece of the puzzle - serde, HTTP clients, JSONSchema (both
//! validation as well as type generation), OpenAPI (both validation as well as
//! type generation), and then finally the Docker client.
//!
//! I want to get around to this at some point (work's already begun on the
//! JSONSchema/OpenAPI bits) but I am only human, and a human with limited time.
//!
//! Word of the wise: If you're doing async HTTP in 2023, use Tokio. Even if you
//! really dislike Tokio, save yourself the headache.
use log::{debug, warn};
use serde::{ser::SerializeSeq, Deserialize, Serialize, Serializer};
use std::collections::HashMap;
use url::Url;

#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error("failed to parse URI: {0}")]
	Uri(#[from] url::ParseError),
	#[error("failed to perform HTTP request: {0}")]
	Http(surf::Error),
	#[error("request returned non-2xx status: {0}")]
	HttpStatus(surf::StatusCode),
	#[error("image failed to build: {0}")]
	DockerBuildFailure(String),
}

impl From<surf::Error> for Error {
	#[inline]
	fn from(value: surf::Error) -> Self {
		Self::Http(value)
	}
}

#[derive(Clone)]
pub struct Docker {
	base: Url,
}

impl Docker {
	pub fn new(path: &str) -> Result<Self, Error> {
		Ok(Self {
			base: Url::parse(path)?,
		})
	}

	fn url<S: AsRef<str>>(&self, path: S) -> String {
		let mut r = self.base.clone();
		r.set_path(path.as_ref());
		r.as_str().to_string()
	}

	pub async fn check_image(&self, id: &str) -> Result<(), Error> {
		let res = surf::get(self.url(format!("/v1.43/images/{id}/json")))
			.send()
			.await?;

		res.status().ok()
	}

	pub async fn create_container(&self, options: &CreateContainer) -> Result<String, Error> {
		let mut res = surf::post(self.url("/v1.43/containers/create"))
			.body_json(options)?
			.send()
			.await?;

		res.status().ok()?;

		let payload: CreateContainerResponse = res.body_json().await?;

		for warning in payload.warnings {
			warn!("docker: create container: {warning}");
		}

		Ok(payload.id)
	}
}

#[derive(Debug, Default)]
pub struct Args(HashMap<String, String>);

impl Args {
	#[inline]
	pub fn new() -> Self {
		Default::default()
	}

	pub fn add(mut self, k: String, v: String) -> Self {
		self.0.insert(k, v);
		self
	}
}

impl Serialize for Args {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
		for (k, v) in &self.0 {
			seq.serialize_element(&format!("{k}={v}"))?;
		}
		seq.end()
	}
}

#[derive(Debug, Default)]
pub struct Map(HashMap<String, String>);

impl Map {
	#[inline]
	pub fn new() -> Self {
		Default::default()
	}

	pub fn add(mut self, k: String, v: String) -> Self {
		self.0.insert(k, v);
		self
	}
}

impl Serialize for Map {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		self.0.serialize(serializer)
	}
}

#[derive(Debug, Default)]
pub struct Binds(pub Vec<(String, String, Option<String>)>);

impl Serialize for Binds {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
		for (src, dst, maybe_opts) in &self.0 {
			if let Some(opts) = maybe_opts {
				seq.serialize_element(&format!("{src}:{dst}:{opts}"))?;
			} else {
				seq.serialize_element(&format!("{src}:{dst}"))?;
			}
		}
		seq.end()
	}
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "PascalCase")]
pub struct CreateContainer {
	pub attach_stdout: Option<bool>,
	pub attach_stderr: Option<bool>,
	pub env: Option<Args>,
	pub image: String,
	pub labels: Option<Map>,
	pub host_config: Option<HostConfig>,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "PascalCase")]
pub struct HostConfig {
	binds: Option<Binds>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "PascalCase")]
struct CreateContainerResponse {
	id: String,
	warnings: Vec<String>,
}

trait StatusCodeCheck {
	fn ok(&self) -> Result<(), Error>;
}

impl StatusCodeCheck for surf::StatusCode {
	fn ok(&self) -> Result<(), Error> {
		if self.is_success() {
			Ok(())
		} else {
			Err(Error::HttpStatus(*self))
		}
	}
}
