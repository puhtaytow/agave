//! Transaction conformance harness.

use {
    super::{context::TxnContext, effects::TxnEffects},
    crate::{
        conformance::{callback::DefaultCallback, setup::program_runtime_environments},
        transaction_processing_result::{ProcessedTransaction, TransactionProcessingResult},
        transaction_processor::{
            ExecutionRecordingConfig, TransactionBatchProcessor, TransactionProcessingConfig,
            TransactionProcessingEnvironment,
        },
    },
    solana_account::{Account, AccountSharedData, ReadableAccount},
    solana_address_lookup_table_interface::{error::AddressLookupError, state::AddressLookupTable},
    solana_clock::{MAX_PROCESSING_AGE, Slot},
    solana_fee::{FeeFeatures, calculate_fee_details},
    solana_hash::Hash,
    solana_message::{
        AddressLoader, SanitizedMessage,
        v0::{LoadedAddresses, MessageAddressTableLookup},
    },
    solana_nonce::state::DurableNonce,
    solana_nonce_account::verify_nonce_account,
    solana_program_runtime::{
        execution_budget::SVMTransactionExecutionBudget,
        loaded_programs::{BlockRelation, ForkGraph},
        program_cache_entry::ProgramCacheEntry,
        solana_sbpf::program::BuiltinFunctionDefinition,
    },
    solana_pubkey::Pubkey,
    solana_runtime_transaction::{
        runtime_transaction::RuntimeTransaction, transaction_meta::TransactionMeta,
    },
    solana_sdk_ids::{
        bpf_loader, bpf_loader_deprecated, bpf_loader_upgradeable, compute_budget, native_loader,
        system_program,
    },
    solana_svm_callback::{InvokeContextCallback, TransactionProcessingCallback},
    solana_svm_transaction::svm_message::{SVMMessage, SVMStaticMessage},
    solana_svm_type_overrides::sync::{Arc, RwLock},
    solana_transaction::{sanitized::MessageHash, sanitized::SanitizedTransaction},
    solana_transaction_error::{AddressLoaderError, TransactionError, TransactionResult},
    std::{cmp::Ordering, collections::HashSet},
};
#[cfg(feature = "conformance")]
use {
    crate::conformance::callback::ConformanceCallback,
    agave_feature_set::virtual_address_space_adjustments,
    agave_precompiles::is_precompile,
    prost::Message,
    protosol::protos::{TxnContext as ProtoTxnContext, TxnResult as ProtoTxnResult},
    std::ffi::c_int,
};

const DEPLOYMENT_SLOT: Slot = 0;

/// Execute a single transaction against the Solana VM with default
/// (no-precompile) callback behavior.
pub fn execute_txn(input: &TxnContext) -> TxnEffects {
    execute_txn_with_callback(input, &DefaultCallback)
}

/// Execute a single transaction against the Solana VM with custom invoke
/// callback behavior.
pub fn execute_txn_with_callback<C: InvokeContextCallback>(
    input: &TxnContext,
    invoke_callback: &C,
) -> TxnEffects {
    let callback = TxnCallback {
        accounts: &input.accounts,
        epoch_total_stake: input.epoch_total_stake,
        invoke_callback,
    };

    let sanitized_tx = match sanitize_transaction(input) {
        Ok(transaction) => transaction,
        Err(err) => return TxnEffects::from_unprocessed_error(err),
    };
    let sanitized_message = sanitized_tx.message().clone();
    let check_result = check_transaction(input, &sanitized_tx);

    let (batch_processor, _fork_graph) = new_batch_processor(input, &callback);
    let processing_environment = transaction_processing_environment(input);
    let processing_config = TransactionProcessingConfig {
        recording_config: ExecutionRecordingConfig {
            enable_cpi_recording: false,
            enable_log_recording: true,
            enable_return_data_recording: true,
            enable_transaction_balance_recording: false,
        },
        limit_to_load_programs: true,
        ..Default::default()
    };

    let output = batch_processor.load_and_execute_sanitized_transactions(
        &callback,
        &[sanitized_tx],
        vec![check_result],
        &processing_environment,
        &processing_config,
    );

    output
        .processing_results
        .into_iter()
        .next()
        .map(|result| TxnEffects::from_processing_result(result, &sanitized_message))
        .unwrap_or_else(|| TxnEffects::from_unprocessed_error(TransactionError::SanitizeFailure))
}

#[cfg(feature = "conformance")]
pub fn execute_txn_proto(input: ProtoTxnContext) -> ProtoTxnResult {
    let context = TxnContext::from(input);
    let virtual_address_space_adjustments_active = context
        .feature_set
        .is_active(&virtual_address_space_adjustments::id());
    let mut effects = execute_txn_with_callback(&context, &ConformanceCallback);

    if let Err(TransactionError::InstructionError(
        index,
        solana_instruction::error::InstructionError::Custom(_),
    )) = &effects.status
    {
        if instruction_is_precompile(*index, &context.transaction.message) {
            effects.status = Err(TransactionError::InstructionError(
                *index,
                solana_instruction::error::InstructionError::Custom(0),
            ));
        }
    }

    let cu_avail = effects.cu_avail;
    let has_err = effects.status.is_err();
    let mut result: ProtoTxnResult = effects.into();
    direct_mapping_handle_cu_exhaustion(
        virtual_address_space_adjustments_active,
        cu_avail,
        has_err,
        result
            .modified_accounts
            .iter_mut()
            .map(|account| &mut account.data),
    );

    result
}

/// Due to how Firedancer's VM CU accounting works, when
/// virtual_address_space_adjustments is enabled and execution fails with the
/// CU meter exhausted, account data cannot be compared with Agave.
#[cfg(feature = "conformance")]
fn direct_mapping_handle_cu_exhaustion<'a>(
    virtual_address_space_adjustments_active: bool,
    cu_avail: u64,
    has_err: bool,
    account_data: impl IntoIterator<Item = &'a mut Vec<u8>>,
) {
    if virtual_address_space_adjustments_active && cu_avail == 0 && has_err {
        for data in account_data {
            data.clear();
        }
    }
}

#[cfg(feature = "conformance")]
fn instruction_is_precompile(
    instruction_error_index: u8,
    message: &solana_message::VersionedMessage,
) -> bool {
    message
        .instructions()
        .get(usize::from(instruction_error_index))
        .and_then(|instruction| {
            message
                .static_account_keys()
                .get(usize::from(instruction.program_id_index))
        })
        .is_some_and(|program_id| is_precompile(program_id, |_| true))
}

/// # Safety
///
/// `in_ptr` must point to `in_sz` initialized bytes. `out_ptr` must point
/// to a writable buffer of at least `*out_psz` bytes. On return, `*out_psz`
/// is updated to the number of bytes written.
#[cfg(feature = "conformance")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sol_compat_svm_txn_execute_v1(
    out_ptr: *mut u8,
    out_psz: *mut u64,
    in_ptr: *mut u8,
    in_sz: u64,
) -> c_int {
    if in_ptr.is_null() || out_ptr.is_null() || out_psz.is_null() {
        return 0;
    }
    let in_slice = unsafe { std::slice::from_raw_parts(in_ptr, in_sz as usize) };
    let Ok(txn_context) = ProtoTxnContext::decode(in_slice) else {
        return 0;
    };
    let txn_result = execute_txn_proto(txn_context);
    let out_slice = unsafe { std::slice::from_raw_parts_mut(out_ptr, (*out_psz) as usize) };
    let out_vec = txn_result.encode_to_vec();
    if out_vec.len() > out_slice.len() {
        return 0;
    }
    out_slice[..out_vec.len()].copy_from_slice(&out_vec);
    unsafe {
        *out_psz = out_vec.len() as u64;
    }
    1
}

struct TxnCallback<'a, C> {
    accounts: &'a [(Pubkey, AccountSharedData)],
    epoch_total_stake: u64,
    invoke_callback: &'a C,
}

impl<C: InvokeContextCallback> InvokeContextCallback for TxnCallback<'_, C> {
    fn get_epoch_stake(&self) -> u64 {
        self.epoch_total_stake
    }

    fn get_epoch_stake_for_vote_account(&self, vote_address: &Pubkey) -> u64 {
        self.invoke_callback
            .get_epoch_stake_for_vote_account(vote_address)
    }

    fn is_precompile(&self, program_id: &Pubkey) -> bool {
        self.invoke_callback.is_precompile(program_id)
    }

    fn process_precompile(
        &self,
        program_id: &Pubkey,
        data: &[u8],
        instruction_datas: Vec<&[u8]>,
    ) -> Result<(), solana_precompile_error::PrecompileError> {
        self.invoke_callback
            .process_precompile(program_id, data, instruction_datas)
    }
}

impl<C> TransactionProcessingCallback for TxnCallback<'_, C> {
    fn get_account_shared_data(&self, pubkey: &Pubkey) -> Option<(AccountSharedData, Slot)> {
        self.accounts
            .iter()
            .find(|(key, account)| key == pubkey && account.lamports() > 0)
            .map(|(_, account)| (account.clone(), DEPLOYMENT_SLOT))
            .or_else(|| builtin_account(pubkey).map(|account| (account, DEPLOYMENT_SLOT)))
    }
}

#[derive(Default)]
struct TxnForkGraph;

impl ForkGraph for TxnForkGraph {
    fn relationship(&self, a: Slot, b: Slot) -> BlockRelation {
        match a.cmp(&b) {
            Ordering::Less => BlockRelation::Ancestor,
            Ordering::Equal => BlockRelation::Equal,
            Ordering::Greater => BlockRelation::Descendant,
        }
    }
}

fn sanitize_transaction(
    input: &TxnContext,
) -> TransactionResult<RuntimeTransaction<SanitizedTransaction>> {
    RuntimeTransaction::try_create(
        input.transaction.clone(),
        input
            .message_hash
            .map(MessageHash::Precomputed)
            .unwrap_or(MessageHash::Compute),
        None,
        TxnAddressLoader { input },
        &HashSet::new(),
        input.feature_set.snapshot().limit_instruction_accounts,
    )
}

fn check_transaction(
    input: &TxnContext,
    transaction: &RuntimeTransaction<SanitizedTransaction>,
) -> TransactionResult<crate::account_loader::CheckedTransactionDetails> {
    let config = transaction.transaction_configuration(&input.feature_set)?;
    let fee_details = calculate_fee_details(
        transaction,
        input.lamports_per_signature,
        config.priority_fee_lamports,
        FeeFeatures::from(&input.feature_set),
    );
    let compute_budget_and_limits =
        solana_program_runtime::execution_budget::SVMTransactionExecutionAndFeeBudgetLimits {
            budget: SVMTransactionExecutionBudget {
                compute_unit_limit: u64::from(config.compute_unit_limit),
                heap_size: config.updated_heap_bytes,
                ..SVMTransactionExecutionBudget::new_with_defaults(
                    input.feature_set.snapshot().raise_cpi_nesting_limit_to_8,
                )
            },
            loaded_accounts_data_size_limit: config.loaded_accounts_data_size_limit,
            fee_details,
        };

    if blockhash_is_valid(input, transaction.recent_blockhash()) {
        return Ok(crate::account_loader::CheckedTransactionDetails::new(
            None,
            compute_budget_and_limits,
        ));
    }

    if let Some(nonce_address) = valid_nonce_address(input, transaction) {
        return Ok(crate::account_loader::CheckedTransactionDetails::new(
            Some(nonce_address),
            compute_budget_and_limits,
        ));
    }

    Err(TransactionError::BlockhashNotFound)
}

fn blockhash_is_valid(input: &TxnContext, blockhash: &Hash) -> bool {
    input
        .blockhash_queue
        .iter()
        .rev()
        .take(MAX_PROCESSING_AGE.saturating_add(1))
        .any(|candidate| candidate == blockhash)
}

fn valid_nonce_address(
    input: &TxnContext,
    transaction: &RuntimeTransaction<SanitizedTransaction>,
) -> Option<Pubkey> {
    let next_durable_nonce = DurableNonce::from_blockhash(&input.blockhash);
    if transaction.recent_blockhash() == next_durable_nonce.as_hash() {
        return None;
    }
    let nonce_address = *transaction.get_durable_nonce()?;
    let nonce_account = input
        .accounts
        .iter()
        .find(|(key, account)| key == &nonce_address && account.lamports() > 0)
        .map(|(_, account)| account)?;
    verify_nonce_account(nonce_account, transaction.recent_blockhash()).map(|_| nonce_address)
}

fn transaction_processing_environment(input: &TxnContext) -> TransactionProcessingEnvironment {
    let runtime_features = input.feature_set.runtime_features();
    let compute_budget = solana_compute_budget::compute_budget::ComputeBudget::new_with_defaults(
        runtime_features.raise_cpi_nesting_limit_to_8,
    );
    let environments = program_runtime_environments(&runtime_features, &compute_budget);
    TransactionProcessingEnvironment {
        blockhash: input.blockhash,
        blockhash_lamports_per_signature: input.blockhash_lamports_per_signature,
        alpenglow_migration_succeeded: false,
        epoch_total_stake: input.epoch_total_stake,
        feature_set: runtime_features,
        program_runtime_environments: environments,
        rent: input.rent.clone(),
    }
}

fn new_batch_processor<C: TransactionProcessingCallback>(
    input: &TxnContext,
    callback: &C,
) -> (
    TransactionBatchProcessor<TxnForkGraph>,
    Arc<RwLock<TxnForkGraph>>,
) {
    let runtime_features = input.feature_set.runtime_features();
    let compute_budget = solana_compute_budget::compute_budget::ComputeBudget::new_with_defaults(
        runtime_features.raise_cpi_nesting_limit_to_8,
    );
    let environments = program_runtime_environments(&runtime_features, &compute_budget);
    let fork_graph = Arc::new(RwLock::new(TxnForkGraph));
    let batch_processor = TransactionBatchProcessor::new(
        input.slot,
        input.epoch,
        Arc::downgrade(&fork_graph),
        Some(environments.get_env_for_execution().clone()),
    );
    register_builtins(&batch_processor);
    batch_processor.reset_and_fill_sysvar_cache_entries(callback);
    (batch_processor, fork_graph)
}

fn register_builtins(batch_processor: &TransactionBatchProcessor<TxnForkGraph>) {
    add_builtin(
        batch_processor,
        solana_system_program::id(),
        "system_program",
        solana_system_program::system_processor::Entrypoint::register,
    );
    add_builtin(
        batch_processor,
        bpf_loader_deprecated::id(),
        "solana_bpf_loader_deprecated_program",
        solana_bpf_loader_program::Entrypoint::register,
    );
    add_builtin(
        batch_processor,
        bpf_loader::id(),
        "solana_bpf_loader_program",
        solana_bpf_loader_program::Entrypoint::register,
    );
    add_builtin(
        batch_processor,
        bpf_loader_upgradeable::id(),
        "solana_bpf_loader_upgradeable_program",
        solana_bpf_loader_program::Entrypoint::register,
    );
    add_builtin(
        batch_processor,
        compute_budget::id(),
        "compute_budget_program",
        solana_compute_budget_program::Entrypoint::register,
    );
    #[cfg(feature = "conformance")]
    add_builtin(
        batch_processor,
        solana_vote_program::id(),
        "vote_program",
        solana_vote_program::vote_processor::Entrypoint::register,
    );
    #[cfg(feature = "conformance")]
    add_builtin(
        batch_processor,
        solana_sdk_ids::zk_elgamal_proof_program::id(),
        "zk_elgamal_proof_program",
        solana_zk_elgamal_proof_program::Entrypoint::register,
    );
}

fn add_builtin(
    batch_processor: &TransactionBatchProcessor<TxnForkGraph>,
    program_id: Pubkey,
    name: &'static str,
    register_fn: solana_program_runtime::invoke_context::BuiltinFunctionRegisterer,
) {
    batch_processor.add_builtin(
        program_id,
        ProgramCacheEntry::new_builtin(DEPLOYMENT_SLOT, name.len(), register_fn),
    );
}

fn builtin_account(pubkey: &Pubkey) -> Option<AccountSharedData> {
    let name = builtin_name(pubkey)?;

    Some(AccountSharedData::from(Account {
        lamports: solana_rent::Rent::default().minimum_balance(name.len()),
        data: name.as_bytes().to_vec(),
        owner: native_loader::id(),
        executable: true,
        rent_epoch: 0,
    }))
}

fn builtin_name(pubkey: &Pubkey) -> Option<&'static str> {
    if system_program::check_id(pubkey) {
        return Some("system_program");
    }
    if bpf_loader_deprecated::check_id(pubkey) {
        return Some("solana_bpf_loader_deprecated_program");
    }
    if bpf_loader::check_id(pubkey) {
        return Some("solana_bpf_loader_program");
    }
    if bpf_loader_upgradeable::check_id(pubkey) {
        return Some("solana_bpf_loader_upgradeable_program");
    }
    if compute_budget::check_id(pubkey) {
        return Some("compute_budget_program");
    }
    #[cfg(feature = "conformance")]
    {
        if pubkey == &solana_vote_program::id() {
            return Some("vote_program");
        }
        if solana_sdk_ids::zk_elgamal_proof_program::check_id(pubkey) {
            return Some("zk_elgamal_proof_program");
        }
    }
    None
}

#[derive(Clone, Copy)]
struct TxnAddressLoader<'a> {
    input: &'a TxnContext,
}

impl AddressLoader for TxnAddressLoader<'_> {
    fn load_addresses(
        self,
        lookups: &[MessageAddressTableLookup],
    ) -> Result<LoadedAddresses, AddressLoaderError> {
        let mut loaded_addresses = LoadedAddresses::default();
        for lookup in lookups {
            let table_account = self
                .input
                .accounts
                .iter()
                .find(|(key, account)| key == &lookup.account_key && account.lamports() > 0)
                .map(|(_, account)| account)
                .ok_or(AddressLoaderError::LookupTableAccountNotFound)?;

            if !solana_address_lookup_table_interface::program::check_id(table_account.owner()) {
                return Err(AddressLoaderError::InvalidAccountOwner);
            }

            let lookup_table = AddressLookupTable::deserialize(table_account.data())
                .map_err(|_| AddressLoaderError::InvalidAccountData)?;
            loaded_addresses.writable.extend(
                lookup_table
                    .lookup_iter(
                        self.input.slot,
                        &lookup.writable_indexes,
                        &self.input.slot_hashes,
                    )
                    .map_err(into_address_loader_error)?
                    .collect::<Option<Vec<_>>>()
                    .ok_or(AddressLoaderError::InvalidLookupIndex)?,
            );
            loaded_addresses.readonly.extend(
                lookup_table
                    .lookup_iter(
                        self.input.slot,
                        &lookup.readonly_indexes,
                        &self.input.slot_hashes,
                    )
                    .map_err(into_address_loader_error)?
                    .collect::<Option<Vec<_>>>()
                    .ok_or(AddressLoaderError::InvalidLookupIndex)?,
            );
        }
        Ok(loaded_addresses)
    }
}

fn into_address_loader_error(err: AddressLookupError) -> AddressLoaderError {
    match err {
        AddressLookupError::LookupTableAccountNotFound => {
            AddressLoaderError::LookupTableAccountNotFound
        }
        AddressLookupError::InvalidAccountOwner => AddressLoaderError::InvalidAccountOwner,
        AddressLookupError::InvalidAccountData => AddressLoaderError::InvalidAccountData,
        AddressLookupError::InvalidLookupIndex => AddressLoaderError::InvalidLookupIndex,
    }
}

impl TxnEffects {
    fn from_unprocessed_error(err: TransactionError) -> Self {
        Self {
            executed: false,
            status: Err(err),
            modified_accounts: vec![],
            rollback_accounts: vec![],
            return_data: vec![],
            executed_units: 0,
            fee_details: Default::default(),
            loaded_accounts_data_size: 0,
            logs: vec![],
            cu_avail: 0,
        }
    }

    fn from_processing_result(
        result: TransactionProcessingResult,
        sanitized_message: &SanitizedMessage,
    ) -> Self {
        let executed = result.is_ok();
        match result {
            Ok(ProcessedTransaction::Executed(executed_tx)) => {
                let status = executed_tx.execution_details.status.clone();
                let modified_accounts = executed_tx
                    .loaded_transaction
                    .accounts
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| sanitized_message.is_writable(*index))
                    .map(|(_, (pubkey, account))| (*pubkey, account.clone()))
                    .collect();
                let rollback_accounts = if status.is_err() {
                    executed_tx
                        .loaded_transaction
                        .rollback_accounts
                        .iter()
                        .map(|(pubkey, account)| (*pubkey, account.clone()))
                        .collect()
                } else {
                    vec![]
                };
                let return_data = executed_tx
                    .execution_details
                    .return_data
                    .as_ref()
                    .map(|info| info.data.clone())
                    .unwrap_or_default();
                let logs = executed_tx
                    .execution_details
                    .log_messages
                    .clone()
                    .unwrap_or_default();
                let executed_units = executed_tx.execution_details.executed_units;
                let cu_avail = executed_tx
                    .loaded_transaction
                    .compute_budget
                    .compute_unit_limit
                    .saturating_sub(executed_units);
                Self {
                    executed,
                    status,
                    modified_accounts,
                    rollback_accounts,
                    return_data,
                    executed_units,
                    fee_details: executed_tx.loaded_transaction.fee_details,
                    loaded_accounts_data_size: u64::from(
                        executed_tx.loaded_transaction.loaded_accounts_data_size,
                    ),
                    logs,
                    cu_avail,
                }
            }
            Ok(ProcessedTransaction::FeesOnly(tx)) => Self {
                executed,
                status: Err(tx.load_error.clone()),
                modified_accounts: vec![],
                rollback_accounts: tx
                    .rollback_accounts
                    .iter()
                    .map(|(pubkey, account)| (*pubkey, account.clone()))
                    .collect(),
                return_data: vec![],
                executed_units: 0,
                fee_details: tx.fee_details,
                loaded_accounts_data_size: u64::from(tx.loaded_accounts_data_size),
                logs: vec![],
                cu_avail: 0,
            },
            Err(err) => Self::from_unprocessed_error(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use {
        super::execute_txn,
        crate::conformance::txn::context::TxnContext,
        agave_feature_set::FeatureSet,
        solana_account::{AccountSharedData, ReadableAccount},
        solana_hash::Hash,
        solana_message::Message,
        solana_pubkey::Pubkey,
        solana_rent::Rent,
        solana_sdk_ids::system_program,
        solana_slot_hashes::SlotHashes,
        solana_system_interface::instruction as system_instruction,
        solana_transaction::{Transaction, versioned::VersionedTransaction},
    };

    #[test]
    fn execute_txn_system_transfer() {
        const FEE: u64 = 5_000;
        const TRANSFER: u64 = 1_000_000;

        let payer = Pubkey::new_unique();
        let recipient = Pubkey::new_unique();
        let blockhash = Hash::new_unique();
        let transaction =
            VersionedTransaction::from(Transaction::new_unsigned(Message::new_with_blockhash(
                &[system_instruction::transfer(&payer, &recipient, TRANSFER)],
                Some(&payer),
                &blockhash,
            )));

        let context = TxnContext {
            feature_set: FeatureSet::default(),
            accounts: vec![
                (
                    payer,
                    AccountSharedData::new(FEE.saturating_add(TRANSFER), 0, &system_program::id()),
                ),
                (
                    recipient,
                    AccountSharedData::new(0, 0, &system_program::id()),
                ),
            ],
            transaction,
            message_hash: None,
            blockhash,
            blockhash_queue: vec![blockhash],
            lamports_per_signature: FEE,
            blockhash_lamports_per_signature: FEE,
            epoch_total_stake: 0,
            slot: 0,
            epoch: 0,
            rent: Rent::default(),
            slot_hashes: SlotHashes::default(),
        };

        let effects = execute_txn(&context);

        assert!(effects.status.is_ok(), "{:?}", effects.status);
        assert_eq!(effects.get_modified_account(&payer).unwrap().lamports(), 0);
        assert_eq!(
            effects.get_modified_account(&recipient).unwrap().lamports(),
            TRANSFER
        );
        assert_eq!(effects.fee_details.transaction_fee(), FEE);
    }
}
