use {
    crate::{
        cluster_info_vote_listener::SlotVoteTracker,
        cluster_slots_service::slot_supporters::SlotSupporters,
        consensus::{Stake, ThresholdDecision, VotedStakes},
        replay_stage::SUPERMINORITY_THRESHOLD,
    },
    solana_clock::Slot,
    solana_hash::Hash,
    solana_ledger::blockstore_processor::{ConfirmationProgress, ReplaySlotStats},
    solana_pubkey::Pubkey,
    solana_runtime::{bank::Bank, bank_forks::BankForks},
    solana_vote::vote_account::VoteAccountsHashMap,
    std::{
        collections::{BTreeMap, HashMap, HashSet},
        sync::{Arc, RwLock},
        time::Instant,
    },
};

type VotedSlot = Slot;
type ExpirationSlot = Slot;
pub type LockoutIntervals = BTreeMap<ExpirationSlot, Vec<(VotedSlot, Pubkey)>>;

#[derive(Debug)]
pub struct ValidatorStakeInfo {
    pub validator_vote_pubkey: Pubkey,
    pub stake: u64,
    pub total_epoch_stake: u64,
}

impl Default for ValidatorStakeInfo {
    fn default() -> Self {
        Self {
            stake: 0,
            validator_vote_pubkey: Pubkey::default(),
            total_epoch_stake: 1,
        }
    }
}

impl ValidatorStakeInfo {
    pub fn new(validator_vote_pubkey: Pubkey, stake: u64, total_epoch_stake: u64) -> Self {
        Self {
            validator_vote_pubkey,
            stake,
            total_epoch_stake,
        }
    }
}

pub const RETRANSMIT_BASE_DELAY_MS: u64 = 5_000;
pub const RETRANSMIT_BACKOFF_CAP: u32 = 6;

#[derive(Debug)]
pub struct RetransmitInfo {
    pub(crate) retry_time: Instant,
    pub(crate) retry_iteration: u32,
}

impl RetransmitInfo {
    pub fn reached_retransmit_threshold(&self) -> bool {
        let backoff = std::cmp::min(self.retry_iteration, RETRANSMIT_BACKOFF_CAP);
        let backoff_duration_ms = (1_u64 << backoff) * RETRANSMIT_BASE_DELAY_MS;
        self.retry_time.elapsed().as_millis() > u128::from(backoff_duration_ms)
    }

    pub fn increment_retry_iteration(&mut self) {
        self.retry_iteration = self.retry_iteration.saturating_add(1);
        self.retry_time = Instant::now();
    }
}

pub struct ForkProgress {
    pub is_dead: bool,
    pub fork_stats: ForkStats,
    pub propagated_stats: PropagatedStats,
    pub replay_stats: Arc<RwLock<ReplaySlotStats>>,
    pub replay_progress: Arc<RwLock<ConfirmationProgress>>,
    pub retransmit_info: RetransmitInfo,
    // Note `num_blocks_on_fork` and `num_dropped_blocks_on_fork` only
    // count new blocks replayed since last restart, which won't include
    // blocks already existing in the ledger/before snapshot at start,
    // so these stats do not span all of time
    pub num_blocks_on_fork: u64,
    pub num_dropped_blocks_on_fork: u64,
}

impl ForkProgress {
    pub fn new(
        last_entry: Hash,
        prev_leader_slot: Option<Slot>,
        validator_stake_info: Option<ValidatorStakeInfo>,
        num_blocks_on_fork: u64,
        num_dropped_blocks_on_fork: u64,
    ) -> Self {
        let (
            is_leader_slot,
            propagated_validators_stake,
            propagated_validators,
            is_propagated,
            total_epoch_stake,
        ) = validator_stake_info
            .map(|info| {
                (
                    true,
                    info.stake,
                    vec![info.validator_vote_pubkey].into_iter().collect(),
                    {
                        if info.total_epoch_stake == 0 {
                            true
                        } else {
                            info.stake as f64 / info.total_epoch_stake as f64
                                > SUPERMINORITY_THRESHOLD
                        }
                    },
                    info.total_epoch_stake,
                )
            })
            .unwrap_or((false, 0, HashSet::new(), false, 0));

        Self {
            is_dead: false,
            fork_stats: ForkStats::default(),
            replay_stats: Arc::new(RwLock::new(ReplaySlotStats::default())),
            replay_progress: Arc::new(RwLock::new(ConfirmationProgress::new(last_entry))),
            num_blocks_on_fork,
            num_dropped_blocks_on_fork,
            propagated_stats: PropagatedStats {
                propagated_validators,
                propagated_validators_stake,
                is_propagated,
                is_leader_slot,
                prev_leader_slot,
                total_epoch_stake,
                ..PropagatedStats::default()
            },
            retransmit_info: RetransmitInfo {
                retry_time: Instant::now(),
                retry_iteration: 0u32,
            },
        }
    }

    pub fn new_from_bank(
        bank: &Bank,
        validator_identity: &Pubkey,
        validator_vote_pubkey: &Pubkey,
        prev_leader_slot: Option<Slot>,
        num_blocks_on_fork: u64,
        num_dropped_blocks_on_fork: u64,
    ) -> Self {
        let validator_stake_info = {
            if bank.collector_id() == validator_identity {
                Some(ValidatorStakeInfo::new(
                    *validator_vote_pubkey,
                    bank.epoch_vote_account_stake(validator_vote_pubkey),
                    bank.total_epoch_stake(),
                ))
            } else {
                None
            }
        };

        let mut new_progress = Self::new(
            bank.last_blockhash(),
            prev_leader_slot,
            validator_stake_info,
            num_blocks_on_fork,
            num_dropped_blocks_on_fork,
        );

        if bank.is_frozen() {
            new_progress.fork_stats.bank_hash = Some(bank.hash());
        }
        new_progress
    }
}

#[derive(Debug, Clone, Default)]
pub struct ForkStats {
    pub fork_stake: Stake,
    pub total_stake: Stake,
    pub block_height: u64,
    pub has_voted: bool,
    pub is_recent: bool,
    pub is_empty: bool,
    pub vote_threshold: Vec<ThresholdDecision>,
    pub is_locked_out: bool,
    pub voted_stakes: VotedStakes,
    pub duplicate_confirmed_hash: Option<Hash>,
    pub computed: bool,
    pub lockout_intervals: LockoutIntervals,
    pub bank_hash: Option<Hash>,
    pub my_latest_landed_vote: Option<Slot>,
}

impl ForkStats {
    /// Return fork_weight, i.e. bank_stake over total_stake.
    pub fn fork_weight(&self) -> f64 {
        self.fork_stake as f64 / self.total_stake as f64
    }
}

#[derive(Clone, Default)]
pub struct PropagatedStats {
    pub propagated_validators: HashSet<Pubkey>,
    pub propagated_node_ids: HashSet<Pubkey>,
    pub propagated_validators_stake: u64,
    pub is_propagated: bool,
    pub is_leader_slot: bool,
    pub prev_leader_slot: Option<Slot>,
    pub slot_vote_tracker: Option<Arc<RwLock<SlotVoteTracker>>>,
    pub cluster_slot_pubkeys: Option<Arc<SlotSupporters>>,
    pub total_epoch_stake: u64,
}

impl PropagatedStats {
    pub fn add_vote_pubkey(&mut self, vote_pubkey: Pubkey, stake: u64) {
        if self.propagated_validators.insert(vote_pubkey) {
            self.propagated_validators_stake += stake;
        }
    }

    pub fn add_node_pubkey(&mut self, node_pubkey: &Pubkey, bank: &Bank) {
        if !self.propagated_node_ids.contains(node_pubkey) {
            let node_vote_accounts = bank
                .epoch_vote_accounts_for_node_id(node_pubkey)
                .map(|v| &v.vote_accounts);

            if let Some(node_vote_accounts) = node_vote_accounts {
                self.add_node_pubkey_internal(
                    node_pubkey,
                    node_vote_accounts,
                    bank.epoch_vote_accounts(bank.epoch())
                        .expect("Epoch stakes for bank's own epoch must exist"),
                );
            }
        }
    }

    fn add_node_pubkey_internal(
        &mut self,
        node_pubkey: &Pubkey,
        vote_account_pubkeys: &[Pubkey],
        epoch_vote_accounts: &VoteAccountsHashMap,
    ) {
        self.propagated_node_ids.insert(*node_pubkey);
        for vote_account_pubkey in vote_account_pubkeys.iter() {
            let stake = epoch_vote_accounts
                .get(vote_account_pubkey)
                .map(|(stake, _)| *stake)
                .unwrap_or(0);
            self.add_vote_pubkey(*vote_account_pubkey, stake);
        }
    }
}

#[derive(Default)]
pub struct ProgressMap {
    progress_map: HashMap<Slot, ForkProgress>,
}

impl std::ops::Deref for ProgressMap {
    type Target = HashMap<Slot, ForkProgress>;
    fn deref(&self) -> &Self::Target {
        &self.progress_map
    }
}

impl std::ops::DerefMut for ProgressMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.progress_map
    }
}

impl ProgressMap {
    pub fn insert(&mut self, slot: Slot, fork_progress: ForkProgress) {
        self.progress_map.insert(slot, fork_progress);
    }

    pub fn get_propagated_stats(&self, slot: Slot) -> Option<&PropagatedStats> {
        self.progress_map
            .get(&slot)
            .map(|fork_progress| &fork_progress.propagated_stats)
    }

    pub fn get_propagated_stats_mut(&mut self, slot: Slot) -> Option<&mut PropagatedStats> {
        self.progress_map
            .get_mut(&slot)
            .map(|fork_progress| &mut fork_progress.propagated_stats)
    }

    pub fn get_propagated_stats_must_exist(&self, slot: Slot) -> &PropagatedStats {
        self.get_propagated_stats(slot)
            .unwrap_or_else(|| panic!("slot={slot} must exist in ProgressMap"))
    }

    pub fn get_fork_stats(&self, slot: Slot) -> Option<&ForkStats> {
        self.progress_map
            .get(&slot)
            .map(|fork_progress| &fork_progress.fork_stats)
    }

    pub fn get_fork_stats_mut(&mut self, slot: Slot) -> Option<&mut ForkStats> {
        self.progress_map
            .get_mut(&slot)
            .map(|fork_progress| &mut fork_progress.fork_stats)
    }

    pub fn get_retransmit_info(&self, slot: Slot) -> Option<&RetransmitInfo> {
        self.progress_map
            .get(&slot)
            .map(|fork_progress| &fork_progress.retransmit_info)
    }

    pub fn get_retransmit_info_mut(&mut self, slot: Slot) -> Option<&mut RetransmitInfo> {
        self.progress_map
            .get_mut(&slot)
            .map(|fork_progress| &mut fork_progress.retransmit_info)
    }

    pub fn is_dead(&self, slot: Slot) -> Option<bool> {
        self.progress_map
            .get(&slot)
            .map(|fork_progress| fork_progress.is_dead)
    }

    pub fn get_hash(&self, slot: Slot) -> Option<Hash> {
        self.progress_map
            .get(&slot)
            .and_then(|fork_progress| fork_progress.fork_stats.bank_hash)
    }

    pub fn is_propagated(&self, slot: Slot) -> Option<bool> {
        self.get_propagated_stats(slot)
            .map(|stats| stats.is_propagated)
    }

    pub fn get_latest_leader_slot_must_exist(&self, slot: Slot) -> Option<Slot> {
        let propagated_stats = self.get_propagated_stats_must_exist(slot);
        if propagated_stats.is_leader_slot {
            Some(slot)
        } else {
            propagated_stats.prev_leader_slot
        }
    }

    pub fn get_leader_propagation_slot_must_exist(&self, slot: Slot) -> (bool, Option<Slot>) {
        if let Some(leader_slot) = self.get_latest_leader_slot_must_exist(slot) {
            // If the leader's stats are None (isn't in the
            // progress map), this means that prev_leader slot is
            // rooted, so return true
            (
                self.is_propagated(leader_slot).unwrap_or(true),
                Some(leader_slot),
            )
        } else {
            // prev_leader_slot doesn't exist because already rooted
            // or this validator hasn't been scheduled as a leader
            // yet. In both cases the latest leader is vacuously
            // confirmed
            (true, None)
        }
    }

    pub fn my_latest_landed_vote(&self, slot: Slot) -> Option<Slot> {
        self.progress_map
            .get(&slot)
            .and_then(|s| s.fork_stats.my_latest_landed_vote)
    }

    pub fn set_duplicate_confirmed_hash(&mut self, slot: Slot, hash: Hash) {
        let slot_progress = self.get_mut(&slot).unwrap();
        slot_progress.fork_stats.duplicate_confirmed_hash = Some(hash);
    }

    pub fn is_duplicate_confirmed(&self, slot: Slot) -> Option<bool> {
        self.progress_map
            .get(&slot)
            .map(|s| s.fork_stats.duplicate_confirmed_hash.is_some())
    }

    pub fn get_bank_prev_leader_slot(&self, bank: &Bank) -> Option<Slot> {
        let parent_slot = bank.parent_slot();
        self.get_propagated_stats(parent_slot)
            .map(|stats| {
                if stats.is_leader_slot {
                    Some(parent_slot)
                } else {
                    stats.prev_leader_slot
                }
            })
            .unwrap_or(None)
    }

    pub fn handle_new_root(&mut self, bank_forks: &BankForks) {
        self.progress_map
            .retain(|k, _| bank_forks.get(*k).is_some());
    }

    pub fn log_propagated_stats(&self, slot: Slot, bank_forks: &RwLock<BankForks>) {
        if let Some(stats) = self.get_propagated_stats(slot) {
            info!(
                "Propagated stats: total staked: {}, observed staked: {}, vote pubkeys: {:?}, \
                 node_pubkeys: {:?}, slot: {slot}, epoch: {:?}",
                stats.total_epoch_stake,
                stats.propagated_validators_stake,
                stats.propagated_validators,
                stats.propagated_node_ids,
                bank_forks.read().unwrap().get(slot).map(|x| x.epoch()),
            );
        }
    }
}

#[cfg(test)]
mod test {
    use {super::*, solana_vote::vote_account::VoteAccount};

    #[test]
    fn test_add_vote_pubkey() {
        let mut stats = PropagatedStats::default();
        let mut vote_pubkey = solana_pubkey::new_rand();

        // Add a vote pubkey, the number of references in all_pubkeys
        // should be 2
        stats.add_vote_pubkey(vote_pubkey, 1);
        assert!(stats.propagated_validators.contains(&vote_pubkey));
        assert_eq!(stats.propagated_validators_stake, 1);

        // Adding it again should change no state since the key already existed
        stats.add_vote_pubkey(vote_pubkey, 1);
        assert!(stats.propagated_validators.contains(&vote_pubkey));
        assert_eq!(stats.propagated_validators_stake, 1);

        // Adding another pubkey should succeed
        vote_pubkey = solana_pubkey::new_rand();
        stats.add_vote_pubkey(vote_pubkey, 2);
        assert!(stats.propagated_validators.contains(&vote_pubkey));
        assert_eq!(stats.propagated_validators_stake, 3);
    }

    #[test]
    fn test_add_node_pubkey_internal() {
        let num_vote_accounts = 10;
        let staked_vote_accounts = 5;
        let vote_account_pubkeys: Vec<_> = std::iter::repeat_with(solana_pubkey::new_rand)
            .take(num_vote_accounts)
            .collect();
        let epoch_vote_accounts: HashMap<_, _> = vote_account_pubkeys
            .iter()
            .skip(num_vote_accounts - staked_vote_accounts)
            .map(|pubkey| (*pubkey, (1, VoteAccount::new_random())))
            .collect();

        let mut stats = PropagatedStats::default();
        let mut node_pubkey = solana_pubkey::new_rand();

        // Add a vote pubkey, the number of references in all_pubkeys
        // should be 2
        stats.add_node_pubkey_internal(&node_pubkey, &vote_account_pubkeys, &epoch_vote_accounts);
        assert!(stats.propagated_node_ids.contains(&node_pubkey));
        assert_eq!(
            stats.propagated_validators_stake,
            staked_vote_accounts as u64
        );

        // Adding it again should not change any state
        stats.add_node_pubkey_internal(&node_pubkey, &vote_account_pubkeys, &epoch_vote_accounts);
        assert!(stats.propagated_node_ids.contains(&node_pubkey));
        assert_eq!(
            stats.propagated_validators_stake,
            staked_vote_accounts as u64
        );

        // Adding another pubkey with same vote accounts should succeed, but stake
        // shouldn't increase
        node_pubkey = solana_pubkey::new_rand();
        stats.add_node_pubkey_internal(&node_pubkey, &vote_account_pubkeys, &epoch_vote_accounts);
        assert!(stats.propagated_node_ids.contains(&node_pubkey));
        assert_eq!(
            stats.propagated_validators_stake,
            staked_vote_accounts as u64
        );

        // Adding another pubkey with different vote accounts should succeed
        // and increase stake
        node_pubkey = solana_pubkey::new_rand();
        let vote_account_pubkeys: Vec<_> = std::iter::repeat_with(solana_pubkey::new_rand)
            .take(num_vote_accounts)
            .collect();
        let epoch_vote_accounts: HashMap<_, _> = vote_account_pubkeys
            .iter()
            .skip(num_vote_accounts - staked_vote_accounts)
            .map(|pubkey| (*pubkey, (1, VoteAccount::new_random())))
            .collect();
        stats.add_node_pubkey_internal(&node_pubkey, &vote_account_pubkeys, &epoch_vote_accounts);
        assert!(stats.propagated_node_ids.contains(&node_pubkey));
        assert_eq!(
            stats.propagated_validators_stake,
            2 * staked_vote_accounts as u64
        );
    }

    #[test]
    fn test_is_propagated_status_on_construction() {
        // If the given ValidatorStakeInfo == None, then this is not
        // a leader slot and is_propagated == false
        let progress = ForkProgress::new(Hash::default(), Some(9), None, 0, 0);
        assert!(!progress.propagated_stats.is_propagated);

        // If the stake is zero, then threshold is always achieved
        let progress = ForkProgress::new(
            Hash::default(),
            Some(9),
            Some(ValidatorStakeInfo {
                total_epoch_stake: 0,
                ..ValidatorStakeInfo::default()
            }),
            0,
            0,
        );
        assert!(progress.propagated_stats.is_propagated);

        // If the stake is non zero, then threshold is not achieved unless
        // validator has enough stake by itself to pass threshold
        let progress = ForkProgress::new(
            Hash::default(),
            Some(9),
            Some(ValidatorStakeInfo {
                total_epoch_stake: 2,
                ..ValidatorStakeInfo::default()
            }),
            0,
            0,
        );
        assert!(!progress.propagated_stats.is_propagated);

        // Give the validator enough stake by itself to pass threshold
        let progress = ForkProgress::new(
            Hash::default(),
            Some(9),
            Some(ValidatorStakeInfo {
                stake: 1,
                total_epoch_stake: 2,
                ..ValidatorStakeInfo::default()
            }),
            0,
            0,
        );
        assert!(progress.propagated_stats.is_propagated);

        // Check that the default ValidatorStakeInfo::default() constructs a ForkProgress
        // with is_propagated == false, otherwise propagation tests will fail to run
        // the proper checks (most will auto-pass without checking anything)
        let progress = ForkProgress::new(
            Hash::default(),
            Some(9),
            Some(ValidatorStakeInfo::default()),
            0,
            0,
        );
        assert!(!progress.propagated_stats.is_propagated);
    }

    #[test]
    fn test_is_propagated() {
        let mut progress_map = ProgressMap::default();

        // Insert new ForkProgress for slot 10 (not a leader slot) and its
        // previous leader slot 9 (leader slot)
        progress_map.insert(10, ForkProgress::new(Hash::default(), Some(9), None, 0, 0));
        progress_map.insert(
            9,
            ForkProgress::new(
                Hash::default(),
                None,
                Some(ValidatorStakeInfo::default()),
                0,
                0,
            ),
        );

        // None of these slot have parents which are confirmed
        assert!(!progress_map.get_leader_propagation_slot_must_exist(9).0);
        assert!(!progress_map.get_leader_propagation_slot_must_exist(10).0);

        // Insert new ForkProgress for slot 8 with no previous leader.
        // The previous leader before 8, slot 7, does not exist in
        // progress map, so is_propagated(8) should return true as
        // this implies the parent is rooted
        progress_map.insert(8, ForkProgress::new(Hash::default(), Some(7), None, 0, 0));
        assert!(progress_map.get_leader_propagation_slot_must_exist(8).0);

        // If we set the is_propagated = true, is_propagated should return true
        progress_map
            .get_propagated_stats_mut(9)
            .unwrap()
            .is_propagated = true;
        assert!(progress_map.get_leader_propagation_slot_must_exist(9).0);
        assert!(progress_map.get(&9).unwrap().propagated_stats.is_propagated);

        // Because slot 9 is now confirmed, then slot 10 is also confirmed b/c 9
        // is the last leader slot before 10
        assert!(progress_map.get_leader_propagation_slot_must_exist(10).0);

        // If we make slot 10 a leader slot though, even though its previous
        // leader slot 9 has been confirmed, slot 10 itself is not confirmed
        progress_map
            .get_propagated_stats_mut(10)
            .unwrap()
            .is_leader_slot = true;
        assert!(!progress_map.get_leader_propagation_slot_must_exist(10).0);
    }
}
