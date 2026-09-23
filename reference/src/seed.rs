use sha2::{Digest, Sha256};

use crate::HEADER_BYTES;

/// Computes the epoch seed for a given epoch number.
#[must_use]
pub fn epoch_seed(epoch: u32) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"QelloxHashV1/epoch-seed/v1");
    hasher.update(epoch.to_le_bytes());
    hasher.finalize().into()
}

/// Computes the mining seed from header and nonce.
#[must_use]
pub fn mining_seed(header: &[u8; HEADER_BYTES], nonce: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"QelloxHashV1/mining-seed/v1");
    hasher.update(header);
    hasher.update(nonce.to_le_bytes());
    hasher.finalize().into()
}

/// Expands a 32-byte seed into a larger keystream using repeated SHA-256.
///
/// Output length must be a multiple of 32 bytes.
pub fn expand_seed(seed: &[u8; 32], output: &mut [u8]) {
    assert!(
        output.len() % 32 == 0,
        "output length must be multiple of 32"
    );
    let blocks = output.len() / 32;
    for i in 0..blocks {
        let mut hasher = Sha256::new();
        hasher.update(b"QelloxHashV1/expand/v1");
        hasher.update(seed);
        hasher.update((i as u32).to_le_bytes());
        let hash: [u8; 32] = hasher.finalize().into();
        output[i * 32..(i + 1) * 32].copy_from_slice(&hash);
    }
}

/// Mixes two 32-byte values with a domain-separated hash.
#[must_use]
pub fn mix_hash256(a: &[u8; 32], b: &[u8; 32], domain: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"QelloxHashV1/mix/v1");
    hasher.update(domain);
    hasher.update(a);
    hasher.update(b);
    hasher.finalize().into()
}
