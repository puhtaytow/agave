//! Transaction context (input).

#[cfg(feature = "conformance")]
use {
    crate::conformance::{account_state::account_from_proto, feature_set::feature_set_from_proto},
    protosol::protos::{
        SanitizedTransaction as ProtoSanitizedTransaction,
        TransactionMessage as ProtoTransactionMessage, TxnContext as ProtoTxnContext,
    },
    solana_account::ReadableAccount,
    solana_message::{
        MessageHeader, VersionedMessage,
        compiled_instruction::CompiledInstruction,
        legacy,
        v0::{self, MessageAddressTableLookup},
    },
    solana_program_runtime::sysvar_cache::SysvarCache,
    solana_signature::Signature,
};
use {
    agave_feature_set::FeatureSet,
    solana_account::AccountSharedData,
    solana_clock::{Epoch, Slot},
    solana_hash::Hash,
    solana_pubkey::Pubkey,
    solana_rent::Rent,
    solana_slot_hashes::SlotHashes,
    solana_transaction::versioned::VersionedTransaction,
};

/// Inputs to a single transaction.
pub struct TxnContext {
    pub feature_set: FeatureSet,
    pub accounts: Vec<(Pubkey, AccountSharedData)>,
    pub transaction: VersionedTransaction,
    pub message_hash: Option<Hash>,
    pub blockhash: Hash,
    pub blockhash_queue: Vec<Hash>,
    pub lamports_per_signature: u64,
    pub blockhash_lamports_per_signature: u64,
    pub epoch_total_stake: u64,
    pub slot: Slot,
    pub epoch: Epoch,
    pub rent: Rent,
    pub slot_hashes: SlotHashes,
}

#[cfg(feature = "conformance")]
impl TxnContext {
    fn populate_sysvars(&mut self) {
        let sysvar_cache = sysvar_cache_from_accounts(&self.accounts);
        if let Ok(clock) = sysvar_cache.get_clock() {
            self.slot = clock.slot;
        }
        if let Ok(epoch_schedule) = sysvar_cache.get_epoch_schedule() {
            self.epoch = epoch_schedule.get_epoch(self.slot);
        }
        if let Ok(rent) = sysvar_cache.get_rent() {
            self.rent = (*rent).clone();
        }
        if let Ok(slot_hashes) = sysvar_cache.get_slot_hashes() {
            self.slot_hashes = SlotHashes::new(slot_hashes.slot_hashes());
        }
    }
}

#[cfg(feature = "conformance")]
pub(crate) fn sysvar_cache_from_accounts(accounts: &[(Pubkey, AccountSharedData)]) -> SysvarCache {
    let mut cache = SysvarCache::default();
    cache.fill_missing_entries(|pubkey, set_sysvar| {
        if let Some((_, account)) = accounts
            .iter()
            .find(|(key, account)| key == pubkey && account.lamports() > 0)
        {
            set_sysvar(account.data());
        }
    });
    cache
}

#[cfg(feature = "conformance")]
impl From<ProtoTxnContext> for TxnContext {
    fn from(value: ProtoTxnContext) -> Self {
        let txn_bank = value.bank.as_ref();
        let accounts = value
            .account_shared_data
            .into_iter()
            .filter(|account| account.lamports > 0)
            .map(|account| {
                let (pubkey, account) = account_from_proto(account);
                (pubkey, AccountSharedData::from(account))
            })
            .collect::<Vec<_>>();

        let tx = value.tx.unwrap_or_default();
        let message_hash = hash_from_slice(&tx.message_hash);
        let proto_message = tx.message.clone().unwrap_or_default();
        let transaction = build_versioned_transaction(tx, &proto_message);

        let feature_set = txn_bank
            .and_then(|bank| bank.features.as_ref())
            .map(feature_set_from_proto)
            .unwrap_or_default();
        let blockhash_queue: Vec<Hash> = txn_bank
            .map(|bank| {
                bank.blockhash_queue
                    .iter()
                    .filter_map(|entry| hash_from_slice(&entry.blockhash))
                    .collect()
            })
            .unwrap_or_default();
        let blockhash = blockhash_queue
            .last()
            .copied()
            .unwrap_or(*transaction.message.recent_blockhash());
        let lamports_per_signature = txn_bank
            .map(|bank| u64::from(bank.rbh_lamports_per_signature))
            .unwrap_or_default();
        let epoch_total_stake = txn_bank
            .map(|bank| bank.total_epoch_stake)
            .unwrap_or_default();
        let mut context = TxnContext {
            feature_set,
            accounts,
            transaction,
            message_hash,
            blockhash,
            blockhash_queue,
            lamports_per_signature,
            blockhash_lamports_per_signature: lamports_per_signature,
            epoch_total_stake,
            slot: 0,
            epoch: 0,
            rent: Rent::default(),
            slot_hashes: SlotHashes::default(),
        };
        context.populate_sysvars();
        context
    }
}

#[cfg(feature = "conformance")]
fn build_versioned_transaction(
    tx: ProtoSanitizedTransaction,
    proto_message: &ProtoTransactionMessage,
) -> VersionedTransaction {
    let message = build_versioned_message(proto_message);
    let mut signatures = tx
        .signatures
        .iter()
        .filter_map(|item| <[u8; 64]>::try_from(item.as_slice()).ok())
        .map(Signature::from)
        .collect::<Vec<_>>();
    if signatures.is_empty() {
        signatures.push(Signature::default());
    }
    VersionedTransaction {
        signatures,
        message,
    }
}

#[cfg(feature = "conformance")]
fn build_versioned_message(value: &ProtoTransactionMessage) -> VersionedMessage {
    let header = value
        .header
        .map(|header| MessageHeader {
            num_required_signatures: (header.num_required_signatures as u8).max(1),
            num_readonly_signed_accounts: header.num_readonly_signed_accounts as u8,
            num_readonly_unsigned_accounts: header.num_readonly_unsigned_accounts as u8,
        })
        .unwrap_or(MessageHeader {
            num_required_signatures: 1,
            num_readonly_signed_accounts: 0,
            num_readonly_unsigned_accounts: 0,
        });
    let account_keys = value
        .account_keys
        .iter()
        .filter_map(|key| Pubkey::try_from(key.as_slice()).ok())
        .collect::<Vec<_>>();
    let recent_blockhash = hash_from_slice(&value.recent_blockhash).unwrap_or_default();
    let instructions = value
        .instructions
        .iter()
        .map(|instruction| CompiledInstruction {
            program_id_index: instruction.program_id_index as u8,
            accounts: instruction
                .accounts
                .iter()
                .map(|index| *index as u8)
                .collect(),
            data: instruction.data.clone(),
        })
        .collect::<Vec<_>>();

    if value.is_legacy {
        VersionedMessage::Legacy(legacy::Message {
            header,
            account_keys,
            recent_blockhash,
            instructions,
        })
    } else {
        VersionedMessage::V0(v0::Message {
            header,
            account_keys,
            recent_blockhash,
            instructions,
            address_table_lookups: value
                .address_table_lookups
                .iter()
                .filter_map(|lookup| {
                    Pubkey::try_from(lookup.account_key.as_slice())
                        .ok()
                        .map(|account_key| MessageAddressTableLookup {
                            account_key,
                            writable_indexes: lookup
                                .writable_indexes
                                .iter()
                                .map(|index| *index as u8)
                                .collect(),
                            readonly_indexes: lookup
                                .readonly_indexes
                                .iter()
                                .map(|index| *index as u8)
                                .collect(),
                        })
                })
                .collect(),
        })
    }
}

#[cfg(feature = "conformance")]
fn hash_from_slice(bytes: &[u8]) -> Option<Hash> {
    <[u8; 32]>::try_from(bytes).ok().map(Hash::new_from_array)
}
