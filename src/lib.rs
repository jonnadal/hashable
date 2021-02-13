//! This library provides [`HashMap`] and [`HashSet`] replacements that implement the [`Hash`]
//! trait -- [`HashableHashMap`] and [`HashableHashSet`].
//!
//! # Example
//!
//! The following is rejected by the compiler:
//!
//! ```rust compile_fail
//! let mut inner_set = std::collections::HashSet::new();
//! inner_set.insert("inner value");
//!
//! let mut outer_set = std::collections::HashSet::new();
//! outer_set.insert(inner_set);
//! ```
//!
//! ```text
//! error[E0277]: the trait bound `HashSet<&str>: Hash` is not satisfied
//! ```
//!
//! The error can be resolved by swapping the inner [`HashSet`] with [`HashableHashSet`]:
//!
//! ```rust
//! let mut inner_set = hashable::HashableHashSet::new();
//! inner_set.insert("inner value");
//!
//! let mut outer_set = std::collections::HashSet::new();
//! outer_set.insert(inner_set);
//! ```

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Debug, Formatter};
use std::hash::{Hash, Hasher, BuildHasher};
use std::ops::{Deref, DerefMut};
use std::iter::FromIterator;

// Reuse a buffer to avoid temporary allocations.
thread_local!(static BUFFER: RefCell<Vec<u64>> = RefCell::new(Vec::with_capacity(100)));

/// A [`HashSet`] wrapper that implements [`Hash`] by sorting pre-hashed entries and feeding those back
/// into the passed-in [`Hasher`].
#[derive(Clone)]
pub struct HashableHashSet<V, S = ahash::RandomState>(HashSet<V, S>);

#[cfg(test)]
fn hash<T: Hash>(value: &T) -> u64 {
    let mut hasher = ahash::AHasher::default();
    value.hash(&mut hasher);
    hasher.finish()
}

impl<V> HashableHashSet<V> {
    #[inline]
    pub fn new() -> HashableHashSet<V> {
        Default::default()
    }
}

impl<V, S> HashableHashSet<V, S> {
    #[inline]
    pub fn with_hasher(hasher: S) -> Self {
        HashableHashSet(HashSet::with_hasher(hasher))
    }
}

impl<V: Debug, S> Debug for HashableHashSet<V, S> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.0.fmt(f) // transparent
    }
}

impl<V, S: Default> Default for HashableHashSet<V, S> {
    #[inline]
    fn default() -> HashableHashSet<V, S> {
        HashableHashSet::with_hasher(S::default())
    }
}

impl<V, S> Deref for HashableHashSet<V, S> {
    type Target = HashSet<V, S>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<V, S> DerefMut for HashableHashSet<V, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<V: Hash + Eq, S: BuildHasher> Eq for HashableHashSet<V, S> {}

impl<V: Eq + Hash, S: BuildHasher + Default> FromIterator<V> for HashableHashSet<V, S> {
    fn from_iter<T: IntoIterator<Item=V>>(iter: T) -> Self {
        HashableHashSet(HashSet::from_iter(iter))
    }
}

impl<V: Hash, S> Hash for HashableHashSet<V, S> {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        BUFFER.with(|buffer| {
            // The cached buffer might already be in use farther up the call stack, so the
            // algorithm reverts to a fallback as needed.
            let fallback = RefCell::new(Vec::new());

            let mut buffer = buffer.try_borrow_mut()
                .unwrap_or_else(|_| fallback.borrow_mut());
            buffer.clear();
            buffer.extend(self.0.iter().map(|v| {
                let mut inner_hasher = ahash::AHasher::default();
                v.hash(&mut inner_hasher);
                inner_hasher.finish()
            }));
            buffer.sort_unstable();
            for v in &*buffer {
                hasher.write_u64(*v);
            }
        });
    }
}

impl<'a, V, S> IntoIterator for &'a HashableHashSet<V, S> {
    type Item = &'a V;
    type IntoIter = std::collections::hash_set::Iter<'a, V>;

    #[inline]
    fn into_iter(self) -> std::collections::hash_set::Iter<'a, V> {
        self.0.iter()
    }
}

impl<V: Hash + Eq, S: BuildHasher> PartialEq for HashableHashSet<V, S> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl<V, S> serde::Serialize for HashableHashSet<V, S>
where V: Eq + Hash + serde::Serialize,
      S: BuildHasher,
{
    fn serialize<Ser: serde::Serializer>(&self, ser: Ser) -> Result<Ser::Ok, Ser::Error> {
        self.0.serialize(ser)
    }
}

#[cfg(test)]
mod hashable_hash_set_test {
    use crate::hash;
    use super::HashableHashSet;

    #[test]
    fn different_hash_if_items_differ() {
        let mut set = HashableHashSet::new();
        set.insert("one");
        set.insert("two");
        set.insert("three");
        let fp1 = hash(&set);

        let mut set = HashableHashSet::new();
        set.insert("four");
        set.insert("five");
        set.insert("six");
        let fp2 = hash(&set);

        assert_ne!(fp1, fp2);
    }

    #[test]
    fn insertion_order_is_irrelevant() {
        let mut set = HashableHashSet::new();
        set.insert("one");
        set.insert("two");
        set.insert("three");
        let fp1 = hash(&set);

        let mut set = HashableHashSet::new();
        set.insert("three");
        set.insert("one");
        set.insert("two");
        let fp2 = hash(&set);

        assert_eq!(fp1, fp2);
    }

    #[test]
    fn can_hash_set_of_sets() {
        // This is a regression test for a case that used to cause `hash` to panic.
        let mut set = HashableHashSet::new();
        set.insert({
            let mut set = HashableHashSet::new();
            set.insert("value");
            set
        });
        hash(&set); // No assertion as this test is just checking for a panic.
    }
}

/// A [`HashMap`] wrapper that implements [`Hash`] by sorting pre-hashed entries and feeding those back
/// into the passed-in [`Hasher`].
#[derive(Clone)]
pub struct HashableHashMap<K, V, S = ahash::RandomState>(HashMap<K, V, S>);

impl<K, V> HashableHashMap<K, V> {
    #[inline]
    pub fn new() -> HashableHashMap<K, V, ahash::RandomState> {
        Default::default()
    }
}

impl<K, V, S> HashableHashMap<K, V, S> {
    #[inline]
    pub fn with_hasher(hasher: S) -> Self {
        HashableHashMap(HashMap::with_hasher(hasher))
    }
}

impl<K: Debug, V: Debug, S> Debug for HashableHashMap<K, V, S> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.0.fmt(f) // transparent
    }
}

impl<K, V, S: Default> Default for HashableHashMap<K, V, S> {
    #[inline]
    fn default() -> Self {
        HashableHashMap::with_hasher(S::default())
    }
}


impl<K, V, S> Deref for HashableHashMap<K, V, S> {
    type Target = HashMap<K, V, S>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K, V, S> DerefMut for HashableHashMap<K, V, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<K: Eq + Hash, V: Eq, S: BuildHasher> Eq for HashableHashMap<K, V, S> {}

impl<K: Eq + Hash, V, S: BuildHasher + Default> FromIterator<(K, V)> for HashableHashMap<K, V, S> {
    fn from_iter<T: IntoIterator<Item=(K, V)>>(iter: T) -> Self {
        HashableHashMap(HashMap::from_iter(iter))
    }
}

impl<K: Hash, V: Hash, S> Hash for HashableHashMap<K, V, S> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        BUFFER.with(|buffer| {
            // The cached buffer might already be in use farther up the call stack, so the
            // algorithm reverts to a fallback as needed.
            let fallback = RefCell::new(Vec::new());

            let mut buffer = buffer.try_borrow_mut()
                .unwrap_or_else(|_| fallback.borrow_mut());
            buffer.clear();
            buffer.extend(self.0.iter().map(|(k, v)| {
                let mut inner_hasher = ahash::AHasher::default();
                k.hash(&mut inner_hasher);
                v.hash(&mut inner_hasher);
                inner_hasher.finish()
            }));
            buffer.sort_unstable();
            for hash in &*buffer {
                state.write_u64(*hash);
            }
        });
    }
}

impl<'a, K, V, S> IntoIterator for &'a HashableHashMap<K, V, S> {
    type Item = (&'a K, &'a V);
    type IntoIter = std::collections::hash_map::Iter<'a, K, V>;

    #[inline]
    fn into_iter(self) -> std::collections::hash_map::Iter<'a, K, V> {
        self.0.iter()
    }
}

impl<K: Hash + Eq, V: PartialEq, S: BuildHasher> PartialEq for HashableHashMap<K, V, S> {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq(&other.0)
    }
}

impl<K, V, S> serde::Serialize for HashableHashMap<K, V, S>
where K: Eq + Hash + serde::Serialize,
      V: serde::Serialize,
      S: BuildHasher,
{
    fn serialize<Ser: serde::Serializer>(&self, ser: Ser) -> Result<Ser::Ok, Ser::Error> {
        self.0.serialize(ser)
    }
}

#[cfg(test)]
mod hashable_hash_map_test {
    use crate::hash;
    use super::HashableHashMap;

    #[test]
    fn different_hash_if_items_differ() {
        let mut map = HashableHashMap::new();
        map.insert("one", 1);
        map.insert("two", 2);
        map.insert("three", 3);
        let fp1 = hash(&map);

        // Same keys as the first map (different values).
        let mut map = HashableHashMap::new();
        map.insert("one", 4);
        map.insert("two", 5);
        map.insert("three", 6);
        let fp2 = hash(&map);

        // Same values as the first map (different keys).
        let mut map = HashableHashMap::new();
        map.insert("four", 1);
        map.insert("five", 2);
        map.insert("six", 3);
        let fp3 = hash(&map);

        assert_ne!(fp1, fp2);
        assert_ne!(fp1, fp3);
        assert_ne!(fp2, fp3);
    }

    #[test]
    fn insertion_order_is_irrelevant() {
        let mut map = HashableHashMap::new();
        map.insert("one", 1);
        map.insert("two", 2);
        map.insert("three", 3);
        let fp1 = hash(&map);

        let mut map = HashableHashMap::new();
        map.insert("three", 3);
        map.insert("one", 1);
        map.insert("two", 2);
        let fp2 = hash(&map);

        assert_eq!(fp1, fp2);
    }

    #[test]
    fn can_hash_map_of_maps() {
        // This is a regression test for a case that used to cause `hash` to panic.
        let mut map = HashableHashMap::new();
        map.insert({
            let mut map = HashableHashMap::new();
            map.insert("key", "value");
            map
        }, "value");
        hash(&map); // No assertion as this test is just checking for a panic.
    }
}
