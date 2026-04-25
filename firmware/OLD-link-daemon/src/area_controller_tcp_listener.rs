use std::{collections::HashSet, fs, io::BufReader, net::SocketAddr, path::Path, sync::Arc};

use anyhow::{Context, Result};
use rustls::{
	CertificateError, DigitallySignedStruct, DistinguishedName, ServerConfig, SignatureScheme,
	client::danger::HandshakeSignatureValid,
	crypto::{self, CryptoProvider},
	pki_types::{CertificateDer, PrivateKeyDer, UnixTime},
	server::danger::{ClientCertVerified, ClientCertVerifier},
};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio_rustls::{TlsAcceptor, server::TlsStream};
use x509_parser::prelude::{FromDer, X509Certificate};

pub type AreaControllerStream = TlsStream<TcpStream>;

#[derive(Clone)]
pub struct AreaControllerListenerConfig {
	acceptor: TlsAcceptor,
}

impl AreaControllerListenerConfig {
	pub fn build(allowed_keys_dir: &Path, tls_cert: &Path, tls_key: &Path) -> Result<Self> {
		let provider = tls_provider();
		let allowed_keys = load_allowed_keys(allowed_keys_dir)?;
		log::info!(
			"loaded {} allowed area-controller public key(s) from '{}'",
			allowed_keys.len(),
			allowed_keys_dir.display()
		);

		let certs = load_certificates(tls_cert)?;
		let key = load_private_key(tls_key)?;
		let verifier = Arc::new(AllowedClientKeysVerifier::new(&provider, allowed_keys));

		let config = ServerConfig::builder_with_provider(provider)
			.with_protocol_versions(&[&rustls::version::TLS13])?
			.with_client_cert_verifier(verifier)
			.with_single_cert(certs, key)
			.context("failed to build TLS server config")?;

		Ok(Self {
			acceptor: TlsAcceptor::from(Arc::new(config)),
		})
	}
}

pub struct AreaControllerTcpListener {
	listener: TcpListener,
	acceptor: TlsAcceptor,
}

impl AreaControllerTcpListener {
	pub async fn bind<A: ToSocketAddrs>(
		config: AreaControllerListenerConfig,
		addr: A,
	) -> Result<Self> {
		let listener = TcpListener::bind(addr)
			.await
			.context("failed to bind area-controller listener")?;

		Ok(Self::from_tcp(listener, config))
	}

	pub fn from_tcp(listener: TcpListener, config: AreaControllerListenerConfig) -> Self {
		Self {
			listener,
			acceptor: config.acceptor,
		}
	}

	pub async fn accept(&self) -> Result<(AreaControllerStream, SocketAddr)> {
		loop {
			let (stream, peer) = self
				.listener
				.accept()
				.await
				.context("failed to accept incoming area-controller connection")?;

			match self.acceptor.accept(stream).await {
				Ok(stream) => return Ok((stream, peer)),
				Err(err) => log::warn!("TLS handshake failed for peer {peer}: {err:#}"),
			}
		}
	}

	pub fn local_addr(&self) -> Result<SocketAddr> {
		self.listener
			.local_addr()
			.context("failed to read area-controller listener address")
	}
}

#[derive(Debug)]
struct AllowedClientKeysVerifier {
	allowed_keys: HashSet<Vec<u8>>,
	root_hints:   Vec<DistinguishedName>,
	algorithms:   crypto::WebPkiSupportedAlgorithms,
}

impl AllowedClientKeysVerifier {
	fn new(provider: &Arc<CryptoProvider>, allowed_keys: HashSet<Vec<u8>>) -> Self {
		Self {
			allowed_keys,
			root_hints: Vec::new(),
			algorithms: provider.signature_verification_algorithms,
		}
	}

	fn verify_allowed_key(
		&self,
		cert: &CertificateDer<'_>,
	) -> std::result::Result<(), rustls::Error> {
		let spki = extract_spki_from_certificate(cert).map_err(|err| {
			log::warn!("rejecting client certificate with invalid encoding: {err:#}");
			rustls::Error::InvalidCertificate(CertificateError::BadEncoding)
		})?;

		if self.allowed_keys.contains(&spki) {
			return Ok(());
		}

		Err(rustls::Error::General(
			"client public key is not present in the allowed key set".to_string(),
		))
	}
}

impl ClientCertVerifier for AllowedClientKeysVerifier {
	fn root_hint_subjects(&self) -> &[DistinguishedName] {
		&self.root_hints
	}

	fn verify_client_cert(
		&self,
		end_entity: &CertificateDer<'_>,
		_intermediates: &[CertificateDer<'_>],
		_now: UnixTime,
	) -> std::result::Result<ClientCertVerified, rustls::Error> {
		self.verify_allowed_key(end_entity)?;
		Ok(ClientCertVerified::assertion())
	}

	fn verify_tls12_signature(
		&self,
		message: &[u8],
		cert: &CertificateDer<'_>,
		dss: &DigitallySignedStruct,
	) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
		crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
	}

	fn verify_tls13_signature(
		&self,
		message: &[u8],
		cert: &CertificateDer<'_>,
		dss: &DigitallySignedStruct,
	) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
		crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
	}

	fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
		self.algorithms.supported_schemes()
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

fn load_allowed_key_file(path: &Path) -> Result<Vec<Vec<u8>>> {
	let pem = fs::read(path)
		.with_context(|| format!("failed to read allowed key file '{}'", path.display()))?;
	let mut reader = BufReader::new(pem.as_slice());
	let mut keys = Vec::new();

	for item in rustls_pemfile::read_all(&mut reader) {
		match item.context("failed to parse allowed-key PEM")? {
			rustls_pemfile::Item::SubjectPublicKeyInfo(spki) => {
				keys.push(spki.as_ref().to_vec());
			}
			rustls_pemfile::Item::X509Certificate(cert) => {
				keys.push(extract_spki_from_certificate(&cert)?);
			}
			_ => {}
		}
	}

	if keys.is_empty() {
		anyhow::bail!(
			"allowed key file '{}' did not contain a public key or certificate",
			path.display()
		);
	}

	Ok(keys)
}

fn load_allowed_keys(dir: &Path) -> Result<HashSet<Vec<u8>>> {
	let mut allowed = HashSet::new();

	for entry in fs::read_dir(dir)
		.with_context(|| format!("failed to read allowed keys directory '{}'", dir.display()))?
	{
		let entry =
			entry.with_context(|| format!("failed to read an entry in '{}'", dir.display()))?;
		let file_type = entry.file_type().with_context(|| {
			format!(
				"failed to determine file type for '{}'",
				entry.path().display()
			)
		})?;
		if !file_type.is_file() {
			continue;
		}

		let path = entry.path();
		for key in load_allowed_key_file(&path)? {
			allowed.insert(key);
		}
	}

	Ok(allowed)
}
