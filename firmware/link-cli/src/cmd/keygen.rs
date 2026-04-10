use std::{
	fs::OpenOptions,
	io::Write,
	path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use rcgen::{
	CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose,
	PKCS_ECDSA_P256_SHA256,
};

#[derive(Debug, clap::Args)]
pub struct KeygenArgs {
	/// Common name to embed in the generated certificate and use for output file names.
	#[clap(value_name = "NAME")]
	name:      String,
	/// Output directory for the generated certificate, key, and public key.
	#[clap(long = "out-dir", short = 'o', default_value = ".")]
	out_dir:   PathBuf,
	/// Overwrite existing files instead of failing.
	#[clap(long = "overwrite", short = 'f')]
	overwrite: bool,
}

struct GeneratedPaths {
	cert:       PathBuf,
	key:        PathBuf,
	public_key: PathBuf,
}

pub fn run(args: KeygenArgs) -> Result<()> {
	let KeygenArgs {
		name,
		out_dir,
		overwrite,
	} = args;

	let mut params = CertificateParams::default();
	params.is_ca = IsCa::NoCa;
	params.distinguished_name.push(DnType::CommonName, &name);
	params
		.key_usages
		.extend([KeyUsagePurpose::DigitalSignature]);
	params.extended_key_usages.extend([
		ExtendedKeyUsagePurpose::ClientAuth,
		ExtendedKeyUsagePurpose::ServerAuth,
	]);

	let key_pair = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)
		.context("failed to generate ECDSA key pair")?;
	let cert = params
		.self_signed(&key_pair)
		.context("failed to generate self-signed certificate")?;
	let paths = write_generated_material(
		&out_dir,
		&name,
		&cert.pem(),
		&key_pair.serialize_pem(),
		&key_pair.public_key_pem(),
		overwrite,
	)?;

	print_generated_paths(&paths);

	Ok(())
}

fn write_file(path: &Path, contents: &str, overwrite: bool) -> Result<()> {
	let mut file = OpenOptions::new()
		.create(true)
		.write(true)
		.truncate(true)
		.create_new(!overwrite)
		.open(path)
		.with_context(|| format!("failed to open '{path}' for writing", path = path.display()))?;

	file.write_all(contents.as_bytes())
		.with_context(|| format!("failed to write '{path}'", path = path.display()))?;

	Ok(())
}

fn write_generated_material(
	out_dir: &Path,
	prefix: &str,
	cert_pem: &str,
	key_pem: &str,
	public_key_pem: &str,
	overwrite: bool,
) -> Result<GeneratedPaths> {
	let cert_path = out_dir.join(format!("{}.cert.pem", prefix));
	let key_path = out_dir.join(format!("{}.key.pem", prefix));
	let public_key_path = out_dir.join(format!("{}.pub.pem", prefix));

	write_file(&cert_path, cert_pem, overwrite)?;
	write_file(&key_path, key_pem, overwrite)?;
	write_file(&public_key_path, public_key_pem, overwrite)?;

	Ok(GeneratedPaths {
		cert:       cert_path,
		key:        key_path,
		public_key: public_key_path,
	})
}

fn print_generated_paths(paths: &GeneratedPaths) {
	let cert = paths.cert.display();
	let key = paths.key.display();
	let public_key = paths.public_key.display();

	println!("certificate: {cert}");
	println!("private key: {key}");
	println!("public key:  {public_key}");
}
