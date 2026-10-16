//! A copy-on-write ordered map for the document's large collections.
//!
//! Commands derive several intermediate documents from one validated head,
//! and the store, plans and hosts keep further copies. Cloning or dropping a
//! `SharedMap` adjusts a reference count; the first mutable access through a
//! shared handle copies the map once (`Arc::make_mut`). Reads, equality,
//! ordering of entries, serialization and every observable value are exactly
//! those of the inner `BTreeMap`, which `Deref` exposes.

use std::collections::BTreeMap;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub(crate) struct SharedMap<K, V>(Arc<BTreeMap<K, V>>);

impl<K, V> SharedMap<K, V> {
    pub(crate) fn new() -> Self {
        Self(Arc::new(BTreeMap::new()))
    }
}

impl<K, V> Default for SharedMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Clone for SharedMap<K, V> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<K, V> Deref for SharedMap<K, V> {
    type Target = BTreeMap<K, V>;
    fn deref(&self) -> &BTreeMap<K, V> {
        &self.0
    }
}

impl<K: Clone, V: Clone> DerefMut for SharedMap<K, V> {
    fn deref_mut(&mut self) -> &mut BTreeMap<K, V> {
        Arc::make_mut(&mut self.0)
    }
}

impl<K, V> From<BTreeMap<K, V>> for SharedMap<K, V> {
    fn from(map: BTreeMap<K, V>) -> Self {
        Self(Arc::new(map))
    }
}

impl<K: Clone, V: Clone> From<SharedMap<K, V>> for BTreeMap<K, V> {
    fn from(map: SharedMap<K, V>) -> Self {
        Arc::unwrap_or_clone(map.0)
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for SharedMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self(Arc::new(iter.into_iter().collect()))
    }
}

impl<K: Clone, V: Clone> IntoIterator for SharedMap<K, V> {
    type Item = (K, V);
    type IntoIter = std::collections::btree_map::IntoIter<K, V>;
    fn into_iter(self) -> Self::IntoIter {
        Arc::unwrap_or_clone(self.0).into_iter()
    }
}

/// For `skip_serializing_if`.
pub(crate) fn is_empty<K, V>(map: &SharedMap<K, V>) -> bool {
    map.0.is_empty()
}

impl<'a, K, V> IntoIterator for &'a SharedMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = std::collections::btree_map::Iter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// Content equality, exactly as for the inner maps (no identity shortcut,
/// so values that are not reflexive compare as they always did).
impl<K: PartialEq, V: PartialEq> PartialEq for SharedMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        *self.0 == *other.0
    }
}

impl<K: Eq, V: Eq> Eq for SharedMap<K, V> {}

impl<K: fmt::Debug, V: fmt::Debug> fmt::Debug for SharedMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<K: Serialize, V: Serialize> Serialize for SharedMap<K, V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, K: Ord + Deserialize<'de>, V: Deserialize<'de>> Deserialize<'de> for SharedMap<K, V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        BTreeMap::deserialize(deserializer).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_until_written_and_compare_by_content() {
        let first: SharedMap<u32, String> =
            BTreeMap::from([(1, "a".to_owned()), (2, "b".to_owned())]).into();
        let mut second = first.clone();
        assert!(Arc::ptr_eq(&first.0, &second.0));
        assert_eq!(first, second);
        second.insert(3, "c".to_owned());
        assert!(!Arc::ptr_eq(&first.0, &second.0));
        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 3);
        assert_ne!(first, second);
        second.remove(&3);
        assert_eq!(first, second);
        let json = serde_json::to_string(&second).unwrap();
        assert_eq!(json, serde_json::to_string(&*first).unwrap());
        let parsed: SharedMap<u32, String> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, first);
        assert_eq!(BTreeMap::from(parsed), *first);
    }

    #[test]
    fn equality_keeps_the_inner_semantics_for_irreflexive_values() {
        let map: SharedMap<u32, f64> = BTreeMap::from([(1, f64::NAN)]).into();
        assert_ne!(map, map.clone());
    }
}
