//! Hash maps with a fixed, fast hasher (FxHash, as in rustc).
//!
//! `std`'s `RandomState` needs OS entropy and atomics; a fixed hasher needs
//! neither and makes iteration order the same on every platform.

use core::hash::{BuildHasherDefault, Hasher};

pub type HashMap<K, V> = hashbrown::HashMap<K, V, BuildHasherDefault<FxHasher>>;
pub type HashSet<K> = hashbrown::HashSet<K, BuildHasherDefault<FxHasher>>;
pub use hashbrown::hash_map;

/// The Fx hash (Firefox / rustc): multiply-rotate, very fast for small keys.
#[derive(Clone, Copy, Default)]
pub struct FxHasher {
    hash: u64,
}

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            self.add(u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut buf = [0u8; 8];
            buf[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(buf) ^ rest.len() as u64);
        }
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(i as u64);
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(i as u64);
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_work() {
        let mut m: HashMap<&str, i32> = HashMap::default();
        m.insert("a", 1);
        m.insert("b", 2);
        assert_eq!(m.get("b"), Some(&2));
        let s: HashSet<u64> = (0..100).collect();
        assert_eq!(s.len(), 100);
    }
}
