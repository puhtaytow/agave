#![allow(dead_code)]
// #![feature(test)]

// macro_rules! DEFINE_NxM_BENCH {
//     ($i:ident, $n:literal, $m:literal) => {
//         mod $i {
//             use super::*;

//             #[bench]
//             fn bench_insert_baseline_hashmap(bencher: &mut Bencher) {
//                 do_bench_insert_baseline_hashmap(bencher, $n, $m);
//             }

//             #[bench]
//             fn bench_insert_bucket_map(bencher: &mut Bencher) {
//                 do_bench_insert_bucket_map(bencher, $n, $m);
//             }
//         }
//     };
// }

// extern crate test;
use {
    bencher::{benchmark_main, Bencher},
    rayon::prelude::*,
    solana_bucket_map::bucket_map::{BucketMap, BucketMapConfig},
    solana_pubkey::Pubkey,
    std::{collections::hash_map::HashMap, sync::RwLock, vec},
    // test::Bencher,
};

type IndexValue = u64;

static BENCH_CASES: &[(usize, usize)] = &[
    (1, 2),
    // (2, 4),
    // (4, 8),
    // (8, 16),
    // (16, 32),
    // (32, 64)
];

// DEFINE_NxM_BENCH!(dim_01x02, BENCH_CASES.0);
// DEFINE_NxM_BENCH!(dim_02x04, 2, 4);
// DEFINE_NxM_BENCH!(dim_04x08, 4, 8);
// DEFINE_NxM_BENCH!(dim_08x16, 8, 16);
// DEFINE_NxM_BENCH!(dim_16x32, 16, 32);
// DEFINE_NxM_BENCH!(dim_32x64, 32, 64);

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
    let (dim_a, dim_b) = BENCH_CASES[0];
    do_bench_insert_baseline_hashmap(bencher, dim_a, dim_b);
}

// #[bench]
fn bench_insert_bucket_map(bencher: &mut Bencher) {
    let (dim_a, dim_b) = BENCH_CASES[0];
    do_bench_insert_bucket_map(bencher, dim_a, dim_b);
}

pub fn benches() -> ::std::vec::Vec<::bencher::TestDescAndFn> {
    use {
        ::bencher::{TestDesc, TestDescAndFn, TestFn},
        std::borrow::Cow,
    };
    let mut benches = ::std::vec::Vec::new();
    benches.push(TestDescAndFn {
        desc: TestDesc {
            name: Cow::from("bench_insert_baseline_hashmap[0]"),
            ignore: false,
        },
        testfn: TestFn::StaticBenchFn(bench_insert_baseline_hashmap),
    });
    benches.push(TestDescAndFn {
        desc: TestDesc {
            name: Cow::from("bench_insert_bucket_map[0]"),
            ignore: false,
        },
        testfn: TestFn::StaticBenchFn(bench_insert_bucket_map),
    });
    benches
}

benchmark_main!(benches);
