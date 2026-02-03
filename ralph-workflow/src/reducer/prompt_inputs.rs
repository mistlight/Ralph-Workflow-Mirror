use sha2::{Digest, Sha256};

pub fn sha256_hex_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex_str(s: &str) -> String {
    sha256_hex_bytes(s.as_bytes())
}
