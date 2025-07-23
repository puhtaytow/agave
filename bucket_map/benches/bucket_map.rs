#![allow(dead_code)] // TODO: remove me

use {
    bencher::{benchmark_main, Bencher, TestDesc, TestDescAndFn, TestFn},
    rayon::prelude::*,
    solana_bucket_map::bucket_map::{BucketMap, BucketMapConfig},
    solana_pubkey::Pubkey,
    std::{borrow::Cow, collections::hash_map::HashMap, sync::RwLock, vec},
};
type IndexValue = u64;

/// Benchmark insert with Hashmap as baseline for N threads inserting M keys each
fn do_bench_insert_baseline_hashmap(bencher: &mut Bencher, n: usize, m: usize) {
    let index = RwLock::new(HashMap::new());
    (0..n).into_par_iter().for_each(|i| {
        let key = Pubkey::new_unique();
        index
            .write()
            .unwrap()
            .insert(key, vec![(i, IndexValue::default())]);
    });
    bencher.iter(|| {
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
fn do_bench_insert_bucket_map(bencher: &mut Bencher, n: usize, m: usize) {
    let index = BucketMap::new(BucketMapConfig::new(n));
    (0..n).into_par_iter().for_each(|i| {
        let key = Pubkey::new_unique();
        index.update(&key, |_| Some((vec![(i, IndexValue::default())], 0)));
    });
    bencher.iter(|| {
        (0..n).into_par_iter().for_each(|_| {
            for j in 0..m {
                let key = Pubkey::new_unique();
                index.update(&key, |_| Some((vec![(j, IndexValue::default())], 0)));
            }
        })
    });
}

// #[bench]
fn bench_insert_baseline_hashmap(bencher: &mut Bencher) {
    let (dim_a, dim_b) = BENCHMARK_CASES[0];
    do_bench_insert_baseline_hashmap(bencher, dim_a, dim_b);
}

// #[bench]
fn bench_insert_bucket_map(bencher: &mut Bencher) {
    let (dim_a, dim_b) = BENCHMARK_CASES[0];
    do_bench_insert_bucket_map(bencher, dim_a, dim_b);
}

static BENCHMARK_CASES: &[(usize, usize)] = &[(1, 2), (2, 4), (4, 8), (8, 16), (16, 32), (32, 64)];

pub fn benches() -> Vec<::bencher::TestDescAndFn> {
    let mut benches = Vec::new();

    BENCHMARK_CASES.iter().enumerate().for_each(|(i, c)| {
        let case_name = format!("{:?}-bench_insert_baseline_hashmap[{:?}", i, c);
        benches.push(TestDescAndFn {
            desc: TestDesc {
                name: Cow::from(case_name),
                ignore: false,
            },
            testfn: TestFn::StaticBenchFn(bench_insert_baseline_hashmap),
        });
    });

    BENCHMARK_CASES.iter().enumerate().for_each(|(i, c)| {
        let case_name = format!("{:?}-bench_insert_bucket_map[{:?}", i, c);
        benches.push(TestDescAndFn {
            desc: TestDesc {
                name: Cow::from(case_name),
                ignore: false,
            },
            testfn: TestFn::StaticBenchFn(bench_insert_bucket_map),
        });
    });

    benches
}

benchmark_main!(benches);
