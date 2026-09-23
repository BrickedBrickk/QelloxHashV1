mod dataset;
mod finalization;
mod program;
mod seed;
mod state;
mod types;
mod vm;

#[cfg(test)]
mod vectors;

pub use dataset::*;
pub use finalization::*;
pub use program::*;
pub use seed::*;
pub use state::*;
pub use types::*;

use sha2::{Digest, Sha256};

/// Domain separator for QelloxHashV1 work function.
pub const QELLOXHASH_V1_DOMAIN: &str = "Qellox/QelloxHashV1";

/// Header size in bytes (version 4 + prev_hash 32 + merkle_root 32 + timestamp 4 + bits 4 + nonce 4).
pub const HEADER_BYTES: usize = 80;

/// Full mining input: header + extra nonce (8 bytes).
pub const MINING_INPUT_BYTES: usize = 88;

/// Number of logical execution lanes per candidate.
pub const LANES: usize = 32;

/// Number of u32 registers per lane.
pub const REGISTERS: usize = 32;

/// Scratchpad size in bytes (48 KiB).
pub const SCRATCHPAD_BYTES: usize = 49_152;

/// Number of u32 words in scratchpad.
pub const SCRATCHPAD_WORDS: usize = SCRATCHPAD_BYTES / 4;

/// Number of execution passes over the generated program.
pub const EXECUTION_PASSES: usize = 4;

/// Dataset item size in bytes (128 bytes, 32 x u32).
pub const DATASET_ITEM_BYTES: usize = 128;

/// Number of u32 words per dataset item.
pub const DATASET_ITEM_WORDS: usize = DATASET_ITEM_BYTES / 4;

/// Initial dataset target size in bytes (~2 GiB).
pub const DATASET_INITIAL_BYTES: u64 = 2_147_483_648;

/// Number of dataset items at initial size.
pub const DATASET_INITIAL_ITEMS: u64 = DATASET_INITIAL_BYTES / DATASET_ITEM_BYTES as u64;

/// Number of parent items used in dataset item generation.
pub const DATASET_PARENTS: usize = 512;

/// Epoch length in blocks.
pub const EPOCH_LENGTH: u64 = 75_000;

/// Number of rounds for cache generation (light cache).
pub const CACHE_ROUNDS: usize = 3;

/// Light cache item size in bytes (64 bytes, 16 x u32).
pub const CACHE_ITEM_BYTES: usize = 64;

/// Light cache initial target size (~16 MiB).
pub const CACHE_INITIAL_BYTES: u64 = 16_777_216;

/// Number of parent items used in light cache generation.
pub const CACHE_PARENTS: usize = 256;

/// Subgroup size for cross-lane mixing operations.
pub const SUBGROUP_SIZE: usize = 8;

/// Number of cross-lane mixing rounds per execution pass.
pub const CROSS_LANE_ROUNDS: usize = 2;

/// Dataset reads per execution pass per lane.
pub const DATASET_READS_PER_PASS: usize = 8;

/// Minimum dependency chain depth for dataset reads.
pub const MIN_DEPENDENCY_DEPTH: usize = 4;

/// Work digest size in bytes.
pub const WORK_DIGEST_BYTES: usize = 32;

/// Final PoW digest size in bytes.
pub const POW_DIGEST_BYTES: usize = 32;

/// Computes the epoch number from a block height.
#[must_use]
pub fn epoch_from_height(height: u64) -> u32 {
    (height / EPOCH_LENGTH) as u32
}

/// Computes the complete QelloxHashV1 proof-of-work.
///
/// Returns the 256-bit PoW digest that must be compared against the target.
#[must_use]
pub fn qelloxhash_v1(header: &[u8; HEADER_BYTES], nonce: u64) -> [u8; POW_DIGEST_BYTES] {
    let epoch = 0u32; // Epoch derived from block height in real usage
    qelloxhash_v1_with_epoch(header, nonce, epoch)
}

/// Computes QelloxHashV1 with an explicit epoch for testing.
#[must_use]
pub fn qelloxhash_v1_with_epoch(
    header: &[u8; HEADER_BYTES],
    nonce: u64,
    epoch: u32,
) -> [u8; POW_DIGEST_BYTES] {
    // 1. Compute seed from header + nonce
    let seed = compute_seed(header, nonce);

    // 2. Generate the dynamic microprogram from the seed
    let program = generate_program(&seed);

    // 3. Initialize VM state
    let mut state = VmState::new(&seed);

    // 4. Execute the VM program across multiple passes
    for pass in 0..EXECUTION_PASSES {
        execute_pass(&mut state, &program, epoch, pass);
    }

    // 5. Reduce state to work digest
    let work_digest = reduce_state(&state);

    // 6. Final cryptographic digest with domain separation
    final_digest(&work_digest)
}

/// Computes the seed from header and nonce.
fn compute_seed(header: &[u8; HEADER_BYTES], nonce: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"QelloxHashV1/seed/v1");
    hasher.update(header);
    hasher.update(nonce.to_le_bytes());
    hasher.finalize().into()
}

/// Reduces the VM state to a 256-bit work digest.
fn reduce_state(state: &VmState) -> [u8; WORK_DIGEST_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(b"QelloxHashV1/reduce/v1");

    // Mix all lanes' final register states
    for lane in 0..LANES {
        for reg in 0..REGISTERS {
            hasher.update(state.registers[lane][reg].to_le_bytes());
        }
    }

    // Mix scratchpad (hash in chunks to avoid huge intermediate)
    let scratchpad_hash = {
        let mut h = Sha256::new();
        h.update(b"QelloxHashV1/scratchpad/v1");
        // Convert u32 words to bytes for hashing
        for &word in state.scratchpad.iter().take(256) {
            h.update(word.to_le_bytes());
        }
        h.finalize()
    };
    hasher.update(scratchpad_hash);

    hasher.finalize().into()
}

/// Verifies that a PoW digest meets the target.
#[must_use]
pub fn verify_pow(pow_digest: &[u8; POW_DIGEST_BYTES], target: &[u8; 32]) -> bool {
    // Compare as big-endian 256-bit integers: pow_digest <= target
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
