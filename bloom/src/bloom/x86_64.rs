#[cfg(target_arch = "x86_64")]
use super::{BloomHashIndex, ConcurrentBloom, Ordering};

#[cfg(target_arch = "x86_64")]
impl<T: BloomHashIndex> ConcurrentBloom<T> {
    pub fn contains_popcnt64(&self, key: &T) -> bool {
        unsafe {
            let mut count = 0;
            for k in &self.keys {
                let (index, mask) = self.pos(key, *k);
                if let Some(bits) = self.bits.get(index) {
                    let bit = bits.load(Ordering::Relaxed) & mask;
                    count += std::arch::x86_64::_popcnt64(bit.try_into().unwrap());
                }
            }
            count > 0
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[cfg(test)]
mod test {
    use {super::*, crate::bloom::Bloom, solana_hash::Hash, solana_sha256_hasher::hash};

    #[test]
    fn test_add_contains_popcnt64() {
        let mut bloom: ConcurrentBloom<Hash> = Bloom::<Hash>::random(100, 0.1, 100).into();
        //known keys to avoid false positives in the test
        bloom.keys = vec![0, 1, 2, 3];

        let key = hash(b"hello");
        assert!(!bloom.contains_popcnt64(&key));
        bloom.add(&key);
        assert!(bloom.contains_popcnt64(&key));

        let key = hash(b"world");
        assert!(!bloom.contains_popcnt64(&key));
        bloom.add(&key);
        assert!(bloom.contains_popcnt64(&key));
    }

    #[test]
    fn test_contains_popcnt64_consistency() {
        let mut bloom: ConcurrentBloom<Hash> = Bloom::<Hash>::random(100, 0.1, 100).into();
        //known keys to avoid false positives in the test
        bloom.keys = vec![0, 1, 2, 3];

        let key = hash(b"hello");
        assert_eq!(bloom.contains(&key), bloom.contains_popcnt64(&key));
        bloom.add(&key);
        assert_eq!(bloom.contains(&key), bloom.contains_popcnt64(&key));

        let key = hash(b"world");
        assert_eq!(bloom.contains(&key), bloom.contains_popcnt64(&key));
        bloom.add(&key);
        assert_eq!(bloom.contains(&key), bloom.contains_popcnt64(&key));
    }
}
