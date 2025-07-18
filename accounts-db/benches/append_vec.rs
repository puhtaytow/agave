use {
    bencher::{benchmark_group, benchmark_main, Bencher},
    rand::{thread_rng, Rng},
    solana_account::{AccountSharedData, ReadableAccount},
    solana_accounts_db::{
        accounts_file::StoredAccountsInfo,
        append_vec::{
            test_utils::{create_test_account, get_append_vec_path},
            AppendVec, StoredMeta,
        },
    },
    solana_clock::Slot,
    std::{
        sync::{Arc, Mutex},
        thread::{sleep, spawn},
        time::Duration,
    },
};

#[cfg(not(any(target_env = "msvc", target_os = "freebsd")))]
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

/// Copy the account metadata, account and hash to the internal buffer.
/// Return the starting offset of the account metadata.
/// After the account is appended, the internal `current_len` is updated.
fn append_account(
    vec: &AppendVec,
    storage_meta: StoredMeta,
    account: &AccountSharedData,
) -> Option<StoredAccountsInfo> {
    let slot_ignored = Slot::MAX;
    let accounts = [(&storage_meta.pubkey, account)];
    let slice = &accounts[..];
    let storable_accounts = (slot_ignored, slice);
    vec.append_accounts(&storable_accounts, 0)
}

fn append_vec_append(b: &mut Bencher) {
    let path = get_append_vec_path("bench_append");
    let vec = AppendVec::new(&path.path, true, 64 * 1024);
    b.iter(|| {
        let (meta, account) = create_test_account(0);
        if append_account(&vec, meta, &account).is_none() {
            vec.reset();
        }
    });
}

fn add_test_accounts(vec: &AppendVec, size: usize) -> Vec<(usize, usize)> {
    (0..size)
        .filter_map(|sample| {
            let (meta, account) = create_test_account(sample);
            append_account(vec, meta, &account).map(|info| (sample, info.offsets[0]))
        })
        .collect()
}

fn append_vec_sequential_read(b: &mut Bencher) {
    let path = get_append_vec_path("seq_read");
    let vec = AppendVec::new(&path.path, true, 64 * 1024);
    let size = 1_000;
    let mut indexes = add_test_accounts(&vec, size);
    b.iter(|| {
        let (sample, pos) = indexes.pop().unwrap();
        println!("reading pos {sample} {pos}");
        vec.get_stored_account_meta_callback(pos, |account| {
            let (_meta, test) = create_test_account(sample);
            assert_eq!(account.data(), test.data());
            indexes.push((sample, pos));
        });
    });
}

fn append_vec_random_read(b: &mut Bencher) {
    let path = get_append_vec_path("random_read");
    let vec = AppendVec::new(&path.path, true, 64 * 1024);
    let size = 1_000;
    let indexes = add_test_accounts(&vec, size);
    b.iter(|| {
        let random_index: usize = thread_rng().gen_range(0..indexes.len());
        let (sample, pos) = &indexes[random_index];
        vec.get_stored_account_meta_callback(*pos, |account| {
            let (_meta, test) = create_test_account(*sample);
            assert_eq!(account.data(), test.data());
        });
    });
}

fn append_vec_concurrent_append_read(b: &mut Bencher) {
    let path = get_append_vec_path("concurrent_read");
    let vec = Arc::new(AppendVec::new(&path.path, true, 1024 * 1024));
    let vec1 = vec.clone();
    let indexes: Arc<Mutex<Vec<(usize, usize)>>> = Arc::new(Mutex::new(vec![]));
    let indexes1 = indexes.clone();
    spawn(move || loop {
        let sample = indexes1.lock().unwrap().len();
        let (meta, account) = create_test_account(sample);
        if let Some(info) = append_account(&vec1, meta, &account) {
            indexes1.lock().unwrap().push((sample, info.offsets[0]))
        } else {
            break;
        }
    });
    while indexes.lock().unwrap().is_empty() {
        sleep(Duration::from_millis(100));
    }
    b.iter(|| {
        let len = indexes.lock().unwrap().len();
        let random_index: usize = thread_rng().gen_range(0..len);
        let (sample, pos) = *indexes.lock().unwrap().get(random_index).unwrap();
        vec.get_stored_account_meta_callback(pos, |account| {
            let (_meta, test) = create_test_account(sample);
            assert_eq!(account.data(), test.data());
        });
    });
}

fn append_vec_concurrent_read_append(b: &mut Bencher) {
    let path = get_append_vec_path("concurrent_read");
    let vec = Arc::new(AppendVec::new(&path.path, true, 1024 * 1024));
    let vec1 = vec.clone();
    let indexes: Arc<Mutex<Vec<(usize, usize)>>> = Arc::new(Mutex::new(vec![]));
    let indexes1 = indexes.clone();
    spawn(move || loop {
        let len = indexes1.lock().unwrap().len();
        if len == 0 {
            continue;
        }
        let random_index: usize = thread_rng().gen_range(0..len + 1);
        let (sample, pos) = *indexes1.lock().unwrap().get(random_index % len).unwrap();
        vec1.get_stored_account_meta_callback(pos, |account| {
            let (_meta, test) = create_test_account(sample);
            assert_eq!(account.data(), test.data());
        });
    });
    b.iter(|| {
        let sample: usize = thread_rng().gen_range(0..256);
        let (meta, account) = create_test_account(sample);
        if let Some(info) = append_account(&vec, meta, &account) {
            indexes.lock().unwrap().push((sample, info.offsets[0]))
        }
    });
}

benchmark_group!(
    benches,
    append_vec_concurrent_read_append,
    append_vec_concurrent_append_read,
    append_vec_random_read,
    append_vec_sequential_read,
    append_vec_append
);
benchmark_main!(benches);
