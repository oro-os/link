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
use log::debug;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error("failed to parse URI: {0}")]
	Uri(#[from] url::ParseError),
	#[error("failed to perform HTTP request: {0}")]
	Http(surf::Error),
	#[error("image failed to build: {0}")]
	DockerBuildFailure(String),
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

	pub async fn build_image(
		&self,
		tarball: Vec<u8>,
		query: &BuildImageQuery,
	) -> Result<(), Error> {
		let mut res = surf::post(self.url("/v1.43/build"))
			.query(query)
			.map_err(Error::Http)?
			.body_bytes(tarball)
			.send()
			.await
			.map_err(Error::Http)?;

		let res_text = res.body_string().await.map_err(Error::Http)?;
		let success = res_text.contains("Successfully built");

		if success {
			Ok(())
		} else {
			Err(Error::DockerBuildFailure(res_text))
		}
	}
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub struct BuildImageQuery {
	#[serde(rename = "t")]
	pub tag: String,
	#[serde(rename = "rm")]
	pub remove_intermediate: bool,
}
