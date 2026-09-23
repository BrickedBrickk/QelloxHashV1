use sha2::{Digest, Sha256};

use crate::POW_DIGEST_BYTES;
use crate::WORK_DIGEST_BYTES;

const FINAL_DOMAIN: &str = "Qellox/QelloxHashV1";

/// Computes the final PoW digest: SHA-256(domain || work_digest).
#[must_use]
pub fn final_digest(work_digest: &[u8; WORK_DIGEST_BYTES]) -> [u8; POW_DIGEST_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(FINAL_DOMAIN.as_bytes());
    hasher.update(work_digest);
    hasher.finalize().into()
}

/// Verifies that a PoW digest meets the target.
///
/// The comparison is done as big-endian 256-bit integers.
#[must_use]
pub fn meets_target(pow_digest: &[u8; POW_DIGEST_BYTES], target: &[u8; 32]) -> bool {
    for i in 0..32 {
        if pow_digest[i] < target[i] {
            return true;
        }
        if pow_digest[i] > target[i] {
            return false;
        }
    }
    true // equal
}
