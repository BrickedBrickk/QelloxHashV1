use sha2::{Digest, Sha256};

use crate::seed::epoch_seed;
use crate::{CACHE_ITEM_BYTES, CACHE_ROUNDS, DATASET_ITEM_WORDS, DATASET_PARENTS};

/// Light cache item (64 bytes = 16 x u32).
type CacheItem = [u32; 16];

/// Dataset item (128 bytes = 32 x u32).
pub type DatasetItem = [u32; DATASET_ITEM_WORDS];

/// Light cache (~16 MiB) used to derive dataset items on demand.
/// Miners precompute the full ~2 GiB dataset; verifiers use only the cache.
pub struct LightCache {
    items: Vec<CacheItem>,
}

impl LightCache {
    /// Builds the light cache for a given epoch.
    #[must_use]
    pub fn new(epoch: u32) -> Self {
        let seed = epoch_seed(epoch);
        let seed_item = seed_to_cache_item(&seed);
        let item_count = cache_item_count(epoch);
        let mut items = Vec::with_capacity(item_count);

        // Generate initial items from seed
        items.push(hash_cache_item(&seed, 0));
        for i in 1..item_count {
            let prev = items[i - 1];
            let mixed = mix_cache_items(&prev, &seed_item, i);
            items.push(mixed);
        }

        // Perform CACHE_ROUNDS of mixing
        for _round in 0..CACHE_ROUNDS {
            for i in 0..item_count {
                let parent_index = items[i][0] as usize % item_count;
                let parent = items[parent_index];
                items[i] = mix_cache_items(&items[i], &parent, i);
            }
        }

        Self { items }
    }

    /// Generates a dataset item on demand from the light cache.
    #[must_use]
    pub fn dataset_item(&self, index: u64) -> DatasetItem {
        let item_count = self.items.len();
        let mut item = [0u32; DATASET_ITEM_WORDS];

        // Start with the cache item at this index
        let base_index = index as usize % item_count;
        item[0..16].copy_from_slice(&self.items[base_index]);

        // Mix DATASET_PARENTS parent items
        for parent_idx in 0..DATASET_PARENTS {
            let mix_value = item[parent_idx % DATASET_ITEM_WORDS]
                .wrapping_add(parent_idx as u32)
                .wrapping_mul(0x0100_0193)
                .wrapping_add(0x811c_9dc5);
            let parent_index = (mix_value as usize) % item_count;
            let parent = self.items[parent_index];

            // Mix parent into item
            for word in 0..DATASET_ITEM_WORDS {
                let cache_word = parent[word % 16];
                item[word] = item[word].wrapping_add(cache_word).rotate_left(7)
                    ^ (parent_idx as u32).wrapping_mul(0x9E37_79B9);
            }
        }

        // Final mixing pass
        for (word, item_word) in item.iter_mut().enumerate() {
            *item_word = (*item_word)
                .wrapping_mul(0x0100_0193)
                .wrapping_add(index as u32)
                .rotate_left((word as u32) & 31);
        }

        item
    }

    /// Returns the number of items in the cache.
    #[must_use]
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    /// Returns the cache item at the given index (for verification).
    #[must_use]
    pub fn cache_item(&self, index: usize) -> &CacheItem {
        &self.items[index]
    }
}

/// Computes the number of light cache items for an epoch.
#[must_use]
pub fn cache_item_count(epoch: u32) -> usize {
    let base = 16_777_216u64; // 16 MiB / 64 bytes
    let growth = 131_072u64; // 128 KiB per epoch
    let bytes = base + growth * epoch as u64;
    (bytes / CACHE_ITEM_BYTES as u64) as usize
}

/// Computes the number of dataset items for an epoch.
#[must_use]
pub fn dataset_item_count(epoch: u32) -> u64 {
    let base = 2_147_483_648u64; // 2 GiB / 128 bytes
    let growth = 8_388_608u64; // 8 MiB per epoch
    (base + growth * epoch as u64) / crate::DATASET_ITEM_BYTES as u64
}

/// Converts a 32-byte seed into a CacheItem for mixing.
fn seed_to_cache_item(seed: &[u8; 32]) -> CacheItem {
    let mut item = [0u32; 16];
    for i in 0..8 {
        item[i * 2] = u32::from_le_bytes([
            seed[i * 4],
            seed[i * 4 + 1],
            seed[i * 4 + 2],
            seed[i * 4 + 3],
        ]);
        item[i * 2 + 1] = item[i * 2].rotate_left(11);
    }
    item
}

/// Hashes a seed into a cache item.
fn hash_cache_item(seed: &[u8; 32], index: usize) -> CacheItem {
    let mut hasher = Sha256::new();
    hasher.update(b"QelloxHashV1/cache-item/v1");
    hasher.update(seed);
    hasher.update((index as u32).to_le_bytes());
    let hash: [u8; 32] = hasher.finalize().into();

    let mut item = [0u32; 16];
    for i in 0..8 {
        item[i * 2] = u32::from_le_bytes([
            hash[i * 4],
            hash[i * 4 + 1],
            hash[i * 4 + 2],
            hash[i * 4 + 3],
        ]);
        // Second word is derived by rotating
        item[i * 2 + 1] = item[i * 2].rotate_left(11);
    }
    item
}

/// Mixes two cache items with a domain-separated hash.
fn mix_cache_items(a: &CacheItem, b: &CacheItem, index: usize) -> CacheItem {
    let mut hasher = Sha256::new();
    hasher.update(b"QelloxHashV1/cache-mix/v1");
    for word in a {
        hasher.update(word.to_le_bytes());
    }
    for word in b {
        hasher.update(word.to_le_bytes());
    }
    hasher.update((index as u32).to_le_bytes());
    let hash: [u8; 32] = hasher.finalize().into();

    let mut result = [0u32; 16];
    for i in 0..8 {
        result[i * 2] = u32::from_le_bytes([
            hash[i * 4],
            hash[i * 4 + 1],
            hash[i * 4 + 2],
            hash[i * 4 + 3],
        ]);
        result[i * 2 + 1] = result[i * 2].rotate_left(11);
    }
    result
}
