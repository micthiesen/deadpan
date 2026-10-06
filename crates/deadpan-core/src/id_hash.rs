//! A small fast hasher for transient identity indexes.
//!
//! Whole-document passes look every node, owner and parent up by identity.
//! These maps are local to one pass, never iterated for output, and never
//! persisted, so their order cannot affect any result.
//!
//! Trust premise: the keys are a validated document's own identities (at most
//! 100,000, each 1–128 ASCII letters, digits, `-` or `_`). A package can come
//! from elsewhere, so a crafted set of colliding identities could slow a pass
//! from linear to quadratic in those bounded counts; it cannot change a
//! result. FNV-1a starts from a per-process random offset, so a collision set
//! computed offline does not carry over between processes, without paying
//! for SipHash on every lookup.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

/// FNV-1a over the written bytes, from a per-process random offset.
#[derive(Clone, Copy)]
pub(crate) struct IdHasher(u64);

impl Default for IdHasher {
    fn default() -> Self {
        static SEED: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
        let seed = *SEED.get_or_init(|| {
            use std::hash::BuildHasher;
            std::collections::hash_map::RandomState::new().hash_one(0x6470_6e31_u64)
        });
        Self(0xcbf2_9ce4_8422_2325 ^ seed)
    }
}

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

pub(crate) type IdBuildHasher = BuildHasherDefault<IdHasher>;
pub(crate) type IdMap<K, V> = HashMap<K, V, IdBuildHasher>;
pub(crate) type IdSet<K> = HashSet<K, IdBuildHasher>;

pub(crate) fn id_map<K, V>(capacity: usize) -> IdMap<K, V> {
    IdMap::with_capacity_and_hasher(capacity, IdBuildHasher::default())
}

pub(crate) fn id_set<K>(capacity: usize) -> IdSet<K> {
    IdSet::with_capacity_and_hasher(capacity, IdBuildHasher::default())
}
