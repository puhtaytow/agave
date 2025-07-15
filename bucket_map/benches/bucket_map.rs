use {
    bencher::{benchmark_group, benchmark_main, Bencher},
    rayon::prelude::*,
    solana_bucket_map::bucket_map::{BucketMap, BucketMapConfig},
    solana_pubkey::Pubkey,
    std::{collections::hash_map::HashMap, sync::RwLock},
};

type IndexValue = u64;

/// Benchmark insert with Hashmap as baseline for N threads inserting M keys each
fn do_bench_insert_baseline_hashmap(b: &mut Bencher, n: usize, m: usize) {
    let index = RwLock::new(HashMap::new());
    (0..n).into_par_iter().for_each(|i| {
        let key = Pubkey::new_unique();
        index
            .write()
            .unwrap()
            .insert(key, vec![(i, IndexValue::default())]);
    });
    b.iter(|| {
        (0..n).into_par_iter().for_each(|_| {
            for j in 0..m {
                let key = Pubkey::new_unique();
                index
                    .write()
                    .unwrap()
                    .insert(key, vec![(j, IndexValue::default())]);
            }
        })
    });
}

/// Benchmark insert with BucketMap with N buckets for N threads inserting M keys each
fn do_bench_insert_bucket_map(b: &mut Bencher, n: usize, m: usize) {
    let index = BucketMap::new(BucketMapConfig::new(n));
    (0..n).into_par_iter().for_each(|i| {
        let key = Pubkey::new_unique();
        index.update(&key, |_| Some((vec![(i, IndexValue::default())], 0)));
    });
    b.iter(|| {
        (0..n).into_par_iter().for_each(|_| {
            for j in 0..m {
                let key = Pubkey::new_unique();
                index.update(&key, |_| Some((vec![(j, IndexValue::default())], 0)));
            }
        })
    });
}

macro_rules! DEFINE_NxM_BENCH {
    ($i:ident, $n:literal, $m:literal) => {
        fn $i(b: &mut Bencher) {
            do_bench_insert_baseline_hashmap(b, $n, $m);
            do_bench_insert_bucket_map(b, $n, $m);
        }
    };
}

DEFINE_NxM_BENCH!(dim_01x02, 1, 2);
DEFINE_NxM_BENCH!(dim_02x04, 2, 4);
DEFINE_NxM_BENCH!(dim_04x08, 4, 8);
DEFINE_NxM_BENCH!(dim_08x16, 8, 16);
DEFINE_NxM_BENCH!(dim_16x32, 16, 32);
DEFINE_NxM_BENCH!(dim_32x64, 32, 64);

benchmark_group!(benches, dim_01x02, dim_02x04, dim_04x08, dim_08x16, dim_16x32, dim_32x64);
benchmark_main!(benches);
