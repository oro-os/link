pub fn unique_id() -> (u32, u32, u32) {
	let uid0 = stm32_metapac::UID.uid(0).read();
	let uid1 = stm32_metapac::UID.uid(1).read();
	let uid2 = stm32_metapac::UID.uid(2).read();
	(uid0, uid1, uid2)
}

pub fn unique_id_sha256() -> [u8; 32] {
	use sha2::Digest;

	let mut sha256 = sha2::Sha256::new();

	let (u1, u2, u3) = unique_id();
	sha256.update(u1.to_be_bytes());
	sha256.update(u2.to_be_bytes());
	sha256.update(u3.to_be_bytes());

	sha256.finalize().into()
}
