//! Transaction context (input).

#[cfg(feature = "conformance")]
use {
    crate::conformance::{account_state::account_from_proto, feature_set::feature_set_from_proto},
    protosol::protos::{
        SanitizedTransaction as ProtoSanitizedTransaction,
        TransactionMessage as ProtoTransactionMessage, TxnContext as ProtoTxnContext,
    },
    solana_account::ReadableAccount,
    solana_address_lookup_table_interface::state::AddressLookupTable,
    solana_message::{
        MessageHeader, SanitizedMessage, SanitizedVersionedMessage, SimpleAddressLoader,
        VersionedMessage,
        compiled_instruction::CompiledInstruction,
        legacy,
        v0::{self, LoadedAddresses, MessageAddressTableLookup},
    },
    solana_sdk_ids::{address_lookup_table, sysvar},
    solana_signature::Signature,
    solana_transaction_error::TransactionError,
    std::collections::HashSet,
};
use {
    agave_feature_set::FeatureSet,
    solana_account::Account,
    solana_clock::{Epoch, Slot},
    solana_hash::Hash,
    solana_pubkey::Pubkey,
    solana_rent::Rent,
    solana_transaction::sanitized::SanitizedTransaction,
};

/// Inputs to a single transaction.
pub struct TxnContext {
    pub bank_feature_set: FeatureSet,
    pub accounts: Vec<(Pubkey, Account)>,
    pub transaction: SanitizedTransaction,
    pub slot: Slot,
    pub epoch: Epoch,
    pub blockhash: Hash,
    pub blockhash_lamports_per_signature: u64,
    pub epoch_total_stake: u64,
    pub rent: Rent,
    pub drop_on_failure: bool,
}

#[cfg(feature = "conformance")]
impl From<ProtoTxnContext> for TxnContext {
    fn from(value: ProtoTxnContext) -> Self {
        let bank = value.bank.as_ref().expect("missing bank context");
        let accounts = value
            .account_shared_data
            .into_iter()
            .filter(|account| account.lamports > 0)
            .map(account_from_proto)
            .collect::<Vec<_>>();
        let clock: Option<solana_clock::Clock> = load_sysvar(&accounts, &sysvar::clock::id());
        let slot = clock.as_ref().map(|clock| clock.slot).unwrap_or_default();
        let epoch = clock.as_ref().map(|clock| clock.epoch).unwrap_or_default();
        let rent = load_sysvar(&accounts, &sysvar::rent::id()).unwrap_or_default();
        let bank_feature_set = bank
            .features
            .as_ref()
            .map(feature_set_from_proto)
            .unwrap_or_default();
        let transaction = build_sanitized_transaction(
            value.tx.as_ref().expect("missing transaction"),
            &accounts,
            slot,
        )
        .expect("invalid transaction");
        let blockhash = bank
            .blockhash_queue
            .last()
            .and_then(|entry| hash_from_bytes(&entry.blockhash).ok())
            .unwrap_or_else(|| *transaction.message().recent_blockhash());

        Self {
            bank_feature_set,
            accounts,
            transaction,
            slot,
            epoch,
            blockhash,
            blockhash_lamports_per_signature: u64::from(bank.rbh_lamports_per_signature),
            epoch_total_stake: bank.total_epoch_stake,
            rent,
            drop_on_failure: false,
        }
    }
}

#[cfg(feature = "conformance")]
fn pubkey_from_bytes(bytes: &[u8]) -> Result<Pubkey, TransactionError> {
    Pubkey::try_from(bytes).map_err(|_| TransactionError::SanitizeFailure)
}

#[cfg(feature = "conformance")]
fn hash_from_bytes(bytes: &[u8]) -> Result<Hash, TransactionError> {
    <[u8; 32]>::try_from(bytes)
        .map(Hash::new_from_array)
        .map_err(|_| TransactionError::SanitizeFailure)
}

#[cfg(feature = "conformance")]
fn signature_from_bytes(bytes: &[u8]) -> Result<Signature, TransactionError> {
    <[u8; 64]>::try_from(bytes)
        .map(Signature::from)
        .map_err(|_| TransactionError::SanitizeFailure)
}

#[cfg(feature = "conformance")]
fn load_sysvar<T: serde::de::DeserializeOwned>(
    accounts: &[(Pubkey, Account)],
    id: &Pubkey,
) -> Option<T> {
    accounts
        .iter()
        .find(|(address, account)| address == id && account.lamports() > 0)
        .and_then(|(_, account)| bincode::deserialize(account.data()).ok())
}

#[cfg(feature = "conformance")]
fn build_versioned_message(
    value: &ProtoTransactionMessage,
) -> Result<VersionedMessage, TransactionError> {
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
        .map(|key| pubkey_from_bytes(key))
        .collect::<Result<Vec<_>, _>>()?;
    let recent_blockhash = hash_from_bytes(&value.recent_blockhash)?;
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

    Ok(if value.is_legacy {
        VersionedMessage::Legacy(legacy::Message {
            header,
            account_keys,
            recent_blockhash,
            instructions,
        })
    } else {
        let address_table_lookups = value
            .address_table_lookups
            .iter()
            .map(|lookup| {
                Ok(MessageAddressTableLookup {
                    account_key: pubkey_from_bytes(&lookup.account_key)?,
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
            .collect::<Result<Vec<_>, TransactionError>>()?;
        VersionedMessage::V0(v0::Message {
            header,
            account_keys,
            recent_blockhash,
            instructions,
            address_table_lookups,
        })
    })
}

#[cfg(feature = "conformance")]
fn load_lookup_addresses(
    lookups: &[MessageAddressTableLookup],
    accounts: &[(Pubkey, Account)],
    slot: Slot,
) -> Result<LoadedAddresses, TransactionError> {
    let slot_hashes = solana_slot_hashes::SlotHashes::default();
    let mut loaded_addresses = LoadedAddresses::default();
    for lookup in lookups {
        let Some((_, account)) = accounts
            .iter()
            .find(|(address, account)| address == &lookup.account_key && account.lamports() > 0)
        else {
            return Err(TransactionError::AddressLookupTableNotFound);
        };
        if account.owner != address_lookup_table::id() {
            return Err(TransactionError::InvalidAddressLookupTableOwner);
        }
        let table = AddressLookupTable::deserialize(account.data())
            .map_err(|_| TransactionError::InvalidAddressLookupTableData)?;
        loaded_addresses.writable.extend(
            table
                .lookup(slot, &lookup.writable_indexes, &slot_hashes)
                .map_err(|err| match err {
                    solana_address_lookup_table_interface::error::AddressLookupError::InvalidLookupIndex => {
                        TransactionError::InvalidAddressLookupTableIndex
                    }
                    solana_address_lookup_table_interface::error::AddressLookupError::LookupTableAccountNotFound => {
                        TransactionError::AddressLookupTableNotFound
                    }
                    _ => TransactionError::InvalidAddressLookupTableData,
                })?,
        );
        loaded_addresses.readonly.extend(
            table
                .lookup(slot, &lookup.readonly_indexes, &slot_hashes)
                .map_err(|err| match err {
                    solana_address_lookup_table_interface::error::AddressLookupError::InvalidLookupIndex => {
                        TransactionError::InvalidAddressLookupTableIndex
                    }
                    solana_address_lookup_table_interface::error::AddressLookupError::LookupTableAccountNotFound => {
                        TransactionError::AddressLookupTableNotFound
                    }
                    _ => TransactionError::InvalidAddressLookupTableData,
                })?,
        );
    }
    Ok(loaded_addresses)
}

#[cfg(feature = "conformance")]
fn build_sanitized_transaction(
    value: &ProtoSanitizedTransaction,
    accounts: &[(Pubkey, Account)],
    slot: Slot,
) -> Result<SanitizedTransaction, TransactionError> {
    let proto_message = value
        .message
        .as_ref()
        .ok_or(TransactionError::SanitizeFailure)?;
    let message = build_versioned_message(proto_message)?;
    let loaded_addresses = match &message {
        VersionedMessage::V0(message) => {
            load_lookup_addresses(&message.address_table_lookups, accounts, slot)?
        }
        _ => LoadedAddresses::default(),
    };
    let message = SanitizedMessage::try_new(
        SanitizedVersionedMessage::try_from(message)
            .map_err(|_| TransactionError::SanitizeFailure)?,
        SimpleAddressLoader::Enabled(loaded_addresses),
        &HashSet::new(),
    )
    .map_err(TransactionError::from)?;

    let message_hash = hash_from_bytes(&value.message_hash)
        .unwrap_or_else(|_| hash_from_bytes(&proto_message.recent_blockhash).unwrap_or_default());
    let mut signatures = value
        .signatures
        .iter()
        .map(|signature| signature_from_bytes(signature))
        .collect::<Result<Vec<_>, _>>()?;
    if signatures.is_empty() {
        signatures.push(Signature::default());
    }

    SanitizedTransaction::try_new_from_fields(message, message_hash, false, signatures)
}
