#![feature(test)]

#[cfg(target_arch = "x86_64")]
extern crate test;
use {
    rand::Rng,
    solana_bloom::bloom::{Bloom, ConcurrentBloom},
    solana_hash::Hash,
    test::Bencher,
};

#[bench]
fn bench_add_hash_atomic(bencher: &mut Bencher) {
    let mut rng = rand::thread_rng();
    let hash_values: Vec<_> = std::iter::repeat_with(Hash::new_unique)
        .take(1200)
        .collect();
    let mut fail = 0;
    bencher.iter(|| {
        let bloom: ConcurrentBloom<_> = Bloom::random(1287, 0.1, 7424).into();
        // Intentionally not using parallelism here, so that this and above
        // benchmark only compare the bit-vector ops.
        // For benchmarking the parallel code, change bellow for loop to:
        //     hash_values.par_iter().for_each(|v| bloom.add(v));
        for hash_value in &hash_values {
            bloom.add(hash_value);
        }
        let index = rng.gen_range(0..hash_values.len());
        if !bloom.contains_popcnt64(&hash_values[index]) {
            fail += 1;
        }
    });
    assert_eq!(fail, 0);
}
