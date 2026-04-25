use std::{fs, io::BufReader, path::Path, sync::Arc};

use anyhow::{Context, Result};
use rustls::{
	CertificateError, DigitallySignedStruct, SignatureScheme,
	client::{
		ClientConfig,
		danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
	},
	crypto::CryptoProvider,
	pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime},
};
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, client::TlsStream};
use x509_parser::prelude::{FromDer, X509Certificate};

use crate::config::DaemonConfig;

pub type DaemonStream = TlsStream<TcpStream>;

#[derive(Clone)]
pub struct DaemonConnectionConfig {
	host:      String,
	connector: TlsConnector,
}

impl DaemonConnectionConfig {
	pub fn build(daemon: &DaemonConfig) -> Result<Self> {
		let provider = tls_provider();
		let pinned_key = load_pinned_key_file(Path::new(&daemon.server_key))?;
		let certs = load_certificates(Path::new(&daemon.client_cert))?;
		let key = load_private_key(Path::new(&daemon.client_key))?;
		let verifier = Arc::new(PinnedServerKeyVerifier::new(&provider, pinned_key));

		let config = ClientConfig::builder_with_provider(provider)
			.with_protocol_versions(&[&rustls::version::TLS13])?
			.dangerous()
			.with_custom_certificate_verifier(verifier)
			.with_client_auth_cert(certs, key)
			.context("failed to build area-controller TLS client config")?;

		let host = daemon.host.clone();
		ServerName::try_from(host.clone())
			.with_context(|| format!("daemon host '{host}' is not a valid TLS server name"))?;

		Ok(Self {
			host,
			connector: TlsConnector::from(Arc::new(config)),
		})
	}

	pub async fn connect(&self, port: u16) -> Result<DaemonStream> {
		let host = self.host.as_str();
		let stream = TcpStream::connect((host, port))
			.await
			.with_context(|| format!("failed to connect to daemon at {host}:{port}"))?;
		let server_name = ServerName::try_from(self.host.clone())
			.with_context(|| format!("daemon host '{host}' is not a valid TLS server name"))?;

		self.connector
			.connect(server_name, stream)
			.await
			.with_context(|| format!("TLS handshake to daemon {host}:{port} failed"))
	}

	pub fn host(&self) -> &str {
		&self.host
	}
}

fn tls_provider() -> Arc<CryptoProvider> {
	let mut provider = rustls::crypto::aws_lc_rs::default_provider();
	provider.cipher_suites =
		vec![rustls::crypto::aws_lc_rs::cipher_suite::TLS13_AES_256_GCM_SHA384];
	Arc::new(provider)
}

fn load_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
	let pem = fs::read(path)
		.with_context(|| format!("failed to read certificate file '{}'", path.display()))?;
	let mut reader = BufReader::new(pem.as_slice());
	let mut certs = Vec::new();

	for item in rustls_pemfile::read_all(&mut reader) {
		if let rustls_pemfile::Item::X509Certificate(cert) =
			item.context("failed to parse certificate PEM")?
		{
			certs.push(cert);
		}
	}

	if certs.is_empty() {
		anyhow::bail!(
			"certificate file '{}' did not contain any certificates",
			path.display()
		);
	}

	Ok(certs)
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>> {
	let pem = fs::read(path)
		.with_context(|| format!("failed to read private key file '{}'", path.display()))?;
	let mut reader = BufReader::new(pem.as_slice());

	for item in rustls_pemfile::read_all(&mut reader) {
		match item.context("failed to parse private key PEM")? {
			rustls_pemfile::Item::Pkcs8Key(key) => return Ok(key.into()),
			rustls_pemfile::Item::Sec1Key(key) => return Ok(key.into()),
			rustls_pemfile::Item::Pkcs1Key(key) => return Ok(key.into()),
			_ => {}
		}
	}

	anyhow::bail!(
		"private key file '{}' did not contain a usable key",
		path.display()
	)
}

fn extract_spki_from_certificate(cert: &CertificateDer<'_>) -> Result<Vec<u8>> {
	let (_, parsed) = X509Certificate::from_der(cert.as_ref())
		.map_err(|err| anyhow::anyhow!("failed to parse X.509 certificate: {err}"))?;
	Ok(parsed.tbs_certificate.subject_pki.raw.to_vec())
}

fn load_pinned_key_file(path: &Path) -> Result<Vec<u8>> {
	let pem = fs::read(path)
		.with_context(|| format!("failed to read pinned key file '{}'", path.display()))?;
	let mut reader = BufReader::new(pem.as_slice());

	for item in rustls_pemfile::read_all(&mut reader) {
		match item.context("failed to parse pinned key PEM")? {
			rustls_pemfile::Item::SubjectPublicKeyInfo(spki) => {
				return Ok(spki.as_ref().to_vec());
			}
			rustls_pemfile::Item::X509Certificate(cert) => {
				return extract_spki_from_certificate(&cert);
			}
			_ => {}
		}
	}

	anyhow::bail!(
		"pinned key file '{}' did not contain a public key or certificate",
		path.display()
	)
}

#[derive(Debug)]
struct PinnedServerKeyVerifier {
	pinned_key: Vec<u8>,
	algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl PinnedServerKeyVerifier {
	fn new(provider: &Arc<CryptoProvider>, pinned_key: Vec<u8>) -> Self {
		Self {
			pinned_key,
			algorithms: provider.signature_verification_algorithms,
		}
	}
}

impl ServerCertVerifier for PinnedServerKeyVerifier {
	fn verify_server_cert(
		&self,
		end_entity: &CertificateDer<'_>,
		_intermediates: &[CertificateDer<'_>],
		_server_name: &ServerName<'_>,
		_ocsp_response: &[u8],
		_now: UnixTime,
	) -> std::result::Result<ServerCertVerified, rustls::Error> {
		let spki = extract_spki_from_certificate(end_entity).map_err(|err| {
			log::warn!("rejecting daemon certificate with invalid encoding: {err:#}");
			rustls::Error::InvalidCertificate(CertificateError::BadEncoding)
		})?;

		if spki != self.pinned_key {
			return Err(rustls::Error::General(
				"daemon public key does not match configured pin".to_string(),
			));
		}

		Ok(ServerCertVerified::assertion())
	}

	fn verify_tls12_signature(
		&self,
		message: &[u8],
		cert: &CertificateDer<'_>,
		dss: &DigitallySignedStruct,
	) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
		rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
	}

	fn verify_tls13_signature(
		&self,
		message: &[u8],
		cert: &CertificateDer<'_>,
		dss: &DigitallySignedStruct,
	) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
		rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
	}

	fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
		self.algorithms.supported_schemes()
	}
}
