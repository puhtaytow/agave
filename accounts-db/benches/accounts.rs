#![allow(clippy::arithmetic_side_effects)]

use {
    bencher::{benchmark_group, benchmark_main, Bencher},
    dashmap::DashMap,
    rand::Rng,
    rayon::iter::{IntoParallelRefIterator, ParallelIterator},
    solana_account::{Account, AccountSharedData, ReadableAccount},
    solana_accounts_db::{
        account_info::{AccountInfo, StorageLocation},
        accounts::{AccountAddressFilter, Accounts},
        accounts_db::{
            test_utils::create_test_accounts, AccountFromStorage, AccountsDb,
            VerifyAccountsHashAndLamportsConfig, ACCOUNTS_DB_CONFIG_FOR_BENCHMARKS,
        },
        accounts_index::ScanConfig,
        ancestors::Ancestors,
    },
    solana_clock::Epoch,
    solana_hash::Hash,
    solana_pubkey::Pubkey,
    solana_sysvar::epoch_schedule::EpochSchedule,
    std::{
        collections::{HashMap, HashSet},
        hint::black_box,
        path::PathBuf,
        sync::{Arc, RwLock},
        thread::Builder,
    },
};

#[cfg(not(any(target_env = "msvc", target_os = "freebsd")))]
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

fn new_accounts_db(account_paths: Vec<PathBuf>) -> AccountsDb {
    AccountsDb::new_with_config(
        account_paths,
        Some(ACCOUNTS_DB_CONFIG_FOR_BENCHMARKS),
        None,
        Arc::default(),
    )
}

fn bench_accounts_hash_bank_hash(b: &mut Bencher) {
    let accounts_db = new_accounts_db(vec![PathBuf::from("bench_accounts_hash_internal")]);
    let accounts = Accounts::new(Arc::new(accounts_db));
    let mut pubkeys: Vec<Pubkey> = vec![];
    let num_accounts = 60_000;
    let slot = 0;
    create_test_accounts(&accounts, &mut pubkeys, num_accounts, slot);
    let ancestors = Ancestors::from(vec![0]);
    let (_, total_lamports) = accounts
        .accounts_db
        .update_accounts_hash_for_tests(0, &ancestors, false, false);
    accounts.add_root(slot);
    accounts.accounts_db.flush_accounts_cache(true, Some(slot));
    b.iter(|| {
        assert!(accounts
            .accounts_db
            .verify_accounts_hash_and_lamports_for_tests(
                0,
                total_lamports,
                VerifyAccountsHashAndLamportsConfig {
                    ancestors: &ancestors,
                    epoch_schedule: &EpochSchedule::default(),
                    epoch: Epoch::default(),
                    ignore_mismatch: false,
                    store_detailed_debug_info: false,
                    use_bg_thread_pool: false,
                }
            )
            .is_ok())
    });
}

fn bench_update_accounts_hash(b: &mut Bencher) {
    solana_logger::setup();
    let accounts_db = new_accounts_db(vec![PathBuf::from("update_accounts_hash")]);
    let accounts = Accounts::new(Arc::new(accounts_db));
    let mut pubkeys: Vec<Pubkey> = vec![];
    create_test_accounts(&accounts, &mut pubkeys, 50_000, 0);
    accounts.accounts_db.add_root_and_flush_write_cache(0);
    let ancestors = Ancestors::from(vec![0]);
    b.iter(|| {
        accounts
            .accounts_db
            .update_accounts_hash_for_tests(0, &ancestors, false, false);
    });
}

fn bench_accounts_delta_hash(b: &mut Bencher) {
    solana_logger::setup();
    let accounts_db = new_accounts_db(vec![PathBuf::from("accounts_delta_hash")]);
    let accounts = Accounts::new(Arc::new(accounts_db));
    let mut pubkeys: Vec<Pubkey> = vec![];
    create_test_accounts(&accounts, &mut pubkeys, 100_000, 0);
    accounts.accounts_db.add_root_and_flush_write_cache(0);
    b.iter(|| {
        accounts.accounts_db.calculate_accounts_delta_hash(0);
    });
}

fn bench_delete_dependencies(b: &mut Bencher) {
    solana_logger::setup();
    let accounts_db = new_accounts_db(vec![PathBuf::from("accounts_delete_deps")]);
    let accounts = Accounts::new(Arc::new(accounts_db));
    let mut old_pubkey = Pubkey::default();
    let zero_account = AccountSharedData::new(0, 0, AccountSharedData::default().owner());
    for i in 0..1000 {
        let pubkey = solana_pubkey::new_rand();
        let account = AccountSharedData::new(i + 1, 0, AccountSharedData::default().owner());
        accounts
            .accounts_db
            .store_for_tests(i, &[(&pubkey, &account)]);
        accounts
            .accounts_db
            .store_for_tests(i, &[(&old_pubkey, &zero_account)]);
        old_pubkey = pubkey;
        accounts.accounts_db.add_root_and_flush_write_cache(i);
    }
    b.iter(|| {
        accounts.accounts_db.clean_accounts_for_tests();
    });
}

fn store_accounts_with_possible_contention<F>(bench_name: &str, b: &mut Bencher, reader_f: F)
where
    F: Fn(&Accounts, &[Pubkey]) + Send + Copy + 'static,
{
    let num_readers = 5;
    let accounts_db = new_accounts_db(vec![PathBuf::from(
        std::env::var("FARF_DIR").unwrap_or_else(|_| "farf".to_string()),
    )
    .join(bench_name)]);
    let accounts = Arc::new(Accounts::new(Arc::new(accounts_db)));
    let num_keys = 1000;
    let slot = 0;

    let pubkeys: Vec<_> = std::iter::repeat_with(solana_pubkey::new_rand)
        .take(num_keys)
        .collect();
    let accounts_data: Vec<_> = std::iter::repeat_n(
        Account {
            lamports: 1,
            ..Default::default()
        }
        .to_account_shared_data(),
        num_keys,
    )
    .collect();
    let storable_accounts: Vec<_> = pubkeys.iter().zip(accounts_data.iter()).collect();
    accounts.store_accounts_cached((slot, storable_accounts.as_slice()));
    accounts.add_root(slot);
    accounts
        .accounts_db
        .flush_accounts_cache_slot_for_tests(slot);

    let pubkeys = Arc::new(pubkeys);
    for i in 0..num_readers {
        let accounts = accounts.clone();
        let pubkeys = pubkeys.clone();
        Builder::new()
            .name(format!("reader{i:02}"))
            .spawn(move || {
                reader_f(&accounts, &pubkeys);
            })
            .unwrap();
    }

    let num_new_keys = 1000;
    b.iter(|| {
        let new_pubkeys: Vec<_> = std::iter::repeat_with(solana_pubkey::new_rand)
            .take(num_new_keys)
            .collect();
        let new_storable_accounts: Vec<_> = new_pubkeys.iter().zip(accounts_data.iter()).collect();
        // Write to a different slot than the one being read from. Because
        // there's a new account pubkey being written to every time, will
        // compete for the accounts index lock on every store
        accounts.store_accounts_cached((slot + 1, new_storable_accounts.as_slice()));
    });
}

fn bench_concurrent_read_write(b: &mut Bencher) {
    store_accounts_with_possible_contention("concurrent_read_write", b, |accounts, pubkeys| {
        let mut rng = rand::thread_rng();
        loop {
            let i = rng.gen_range(0..pubkeys.len());
            black_box(
                accounts
                    .load_without_fixed_root(&Ancestors::default(), &pubkeys[i])
                    .unwrap(),
            );
        }
    })
}

fn bench_concurrent_scan_write(b: &mut Bencher) {
    store_accounts_with_possible_contention("concurrent_scan_write", b, |accounts, _| loop {
        black_box(
            accounts
                .load_by_program(
                    &Ancestors::default(),
                    0,
                    AccountSharedData::default().owner(),
                    &ScanConfig::default(),
                )
                .unwrap(),
        );
    })
}

// #[ignore]
fn bench_dashmap_single_reader_with_n_writers(b: &mut Bencher) {
    let num_readers = 5;
    let num_keys = 10000;
    let map = Arc::new(DashMap::new());
    for i in 0..num_keys {
        map.insert(i, i);
    }
    for _ in 0..num_readers {
        let map = map.clone();
        Builder::new()
            .name("readers".to_string())
            .spawn(move || loop {
                black_box(map.entry(5).or_insert(2));
            })
            .unwrap();
    }
    b.iter(|| {
        for _ in 0..num_keys {
            black_box(map.get(&5).unwrap().value());
        }
    })
}

// #[ignore]
fn bench_rwlock_hashmap_single_reader_with_n_writers(b: &mut Bencher) {
    let num_readers = 5;
    let num_keys = 10000;
    let map = Arc::new(RwLock::new(HashMap::new()));
    for i in 0..num_keys {
        map.write().unwrap().insert(i, i);
    }
    for _ in 0..num_readers {
        let map = map.clone();
        Builder::new()
            .name("readers".to_string())
            .spawn(move || loop {
                black_box(map.write().unwrap().get(&5));
            })
            .unwrap();
    }
    b.iter(|| {
        for _ in 0..num_keys {
            black_box(map.read().unwrap().get(&5));
        }
    })
}

fn setup_bench_dashmap_iter() -> (Arc<Accounts>, DashMap<Pubkey, (AccountSharedData, Hash)>) {
    let accounts_db = new_accounts_db(vec![PathBuf::from(
        std::env::var("FARF_DIR").unwrap_or_else(|_| "farf".to_string()),
    )
    .join("bench_dashmap_par_iter")]);
    let accounts = Arc::new(Accounts::new(Arc::new(accounts_db)));

    let dashmap = DashMap::new();
    let num_keys = std::env::var("NUM_BENCH_KEYS")
        .map(|num_keys| num_keys.parse::<usize>().unwrap())
        .unwrap_or_else(|_| 10000);
    for _ in 0..num_keys {
        dashmap.insert(
            Pubkey::new_unique(),
            (
                AccountSharedData::new(1, 0, AccountSharedData::default().owner()),
                Hash::new_unique(),
            ),
        );
    }

    (accounts, dashmap)
}

fn bench_dashmap_par_iter(b: &mut Bencher) {
    let (accounts, dashmap) = setup_bench_dashmap_iter();

    b.iter(|| {
        black_box(accounts.accounts_db.thread_pool.install(|| {
            dashmap
                .par_iter()
                .map(|cached_account| (*cached_account.key(), cached_account.value().1))
                .collect::<Vec<(Pubkey, Hash)>>()
        }));
    });
}

fn bench_dashmap_iter(b: &mut Bencher) {
    let (_accounts, dashmap) = setup_bench_dashmap_iter();

    b.iter(|| {
        black_box(
            dashmap
                .iter()
                .map(|cached_account| (*cached_account.key(), cached_account.value().1))
                .collect::<Vec<(Pubkey, Hash)>>(),
        );
    });
}

fn bench_load_largest_accounts(b: &mut Bencher) {
    let accounts_db = new_accounts_db(Vec::new());
    let accounts = Accounts::new(Arc::new(accounts_db));
    let mut rng = rand::thread_rng();
    for _ in 0..10_000 {
        let lamports = rng.gen();
        let pubkey = Pubkey::new_unique();
        let account = AccountSharedData::new(lamports, 0, &Pubkey::default());
        accounts
            .accounts_db
            .store_for_tests(0, &[(&pubkey, &account)]);
    }
    accounts.accounts_db.add_root_and_flush_write_cache(0);
    let ancestors = Ancestors::from(vec![0]);
    let bank_id = 0;
    b.iter(|| {
        accounts.load_largest_accounts(
            &ancestors,
            bank_id,
            20,
            &HashSet::new(),
            AccountAddressFilter::Exclude,
            false,
        )
    });
}

fn bench_sort_and_remove_dups(b: &mut Bencher) {
    fn generate_sample_account_from_storage(i: u8) -> AccountFromStorage {
        // offset has to be 8 byte aligned
        let offset = (i as usize) * std::mem::size_of::<u64>();
        AccountFromStorage {
            index_info: AccountInfo::new(StorageLocation::AppendVec(i as u32, offset), i == 0),
            data_len: i as u64,
            pubkey: Pubkey::new_from_array([i; 32]),
        }
    }

    use rand::prelude::*;
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1234);
    let accounts: Vec<_> =
        std::iter::repeat_with(|| generate_sample_account_from_storage(rng.gen::<u8>()))
            .take(1000)
            .collect();

    b.iter(|| AccountsDb::sort_and_remove_dups(&mut accounts.clone()));
}

fn bench_sort_and_remove_dups_no_dups(b: &mut Bencher) {
    fn generate_sample_account_from_storage(i: u8) -> AccountFromStorage {
        // offset has to be 8 byte aligned
        let offset = (i as usize) * std::mem::size_of::<u64>();
        AccountFromStorage {
            index_info: AccountInfo::new(StorageLocation::AppendVec(i as u32, offset), i == 0),
            data_len: i as u64,
            pubkey: Pubkey::new_unique(),
        }
    }

    use rand::prelude::*;
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1234);
    let mut accounts: Vec<_> =
        std::iter::repeat_with(|| generate_sample_account_from_storage(rng.gen::<u8>()))
            .take(1000)
            .collect();

    accounts.shuffle(&mut rng);

    b.iter(|| AccountsDb::sort_and_remove_dups(&mut accounts.clone()));
}

benchmark_group!(
    benches,
    bench_sort_and_remove_dups_no_dups,
    bench_sort_and_remove_dups,
    bench_load_largest_accounts,
    bench_dashmap_iter,
    bench_dashmap_par_iter,
    bench_rwlock_hashmap_single_reader_with_n_writers,
    bench_dashmap_single_reader_with_n_writers,
    bench_concurrent_scan_write,
    bench_concurrent_read_write,
    bench_delete_dependencies,
    bench_accounts_delta_hash,
    bench_update_accounts_hash,
    bench_accounts_hash_bank_hash
);
benchmark_main!(benches);
