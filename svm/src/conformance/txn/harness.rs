//! Transaction conformance harness.

#[cfg(feature = "conformance")]
use {
    crate::conformance::callback::ConformanceCallback,
    prost::Message,
    protosol::protos::{TxnContext as ProtoTxnContext, TxnResult as ProtoTxnResult},
    std::ffi::c_int,
};
use {
    crate::{
        conformance::{
            callback::DefaultCallback,
            programs::add_builtins_to_transaction_batch_processor,
            setup::program_runtime_environments,
            txn::{context::TxnContext, effects::TxnEffects},
        },
        transaction_processing_result::{ProcessedTransaction, TransactionProcessingResult},
        transaction_processor::{
            ExecutionRecordingConfig, TransactionBatchProcessor, TransactionProcessingConfig,
            TransactionProcessingEnvironment,
        },
    },
    solana_account::{AccountSharedData, ReadableAccount},
    solana_clock::Slot,
    solana_compute_budget::compute_budget::ComputeBudget,
    solana_compute_budget_instruction::instructions_processor::process_compute_budget_instructions,
    solana_fee::{FeeFeatures, calculate_fee_details},
    solana_fee_structure::FeeDetails,
    solana_program_runtime::loaded_programs::{BlockRelation, ForkGraph},
    solana_pubkey::Pubkey,
    solana_svm_callback::{AccountState, InvokeContextCallback, TransactionProcessingCallback},
    solana_svm_transaction::svm_message::{SVMMessage, SVMStaticMessage},
    solana_svm_type_overrides::sync::{Arc, RwLock},
    solana_transaction::sanitized::SanitizedTransaction,
    solana_transaction_error::TransactionError,
    std::cmp::Ordering,
};

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

struct TxnCallback<'a, C> {
    input: &'a TxnContext,
    invoke_callback: &'a C,
}

impl<C: InvokeContextCallback> InvokeContextCallback for TxnCallback<'_, C> {
    fn get_epoch_stake(&self) -> u64 {
        self.input.epoch_total_stake
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

impl<C: InvokeContextCallback> TransactionProcessingCallback for TxnCallback<'_, C> {
    fn get_account_shared_data(&self, pubkey: &Pubkey) -> Option<(AccountSharedData, Slot)> {
        self.input
            .accounts
            .iter()
            .find(|(address, account)| address == pubkey && account.lamports() > 0)
            .map(|(_, account)| (AccountSharedData::from(account.clone()), self.input.slot))
    }

    fn inspect_account(&self, _address: &Pubkey, _account_state: AccountState, _is_writable: bool) {
    }
}

fn transaction_check_result(input: &TxnContext) -> crate::account_loader::TransactionCheckResult {
    let runtime_features = input.bank_feature_set.runtime_features();
    let compute_budget_limits = process_compute_budget_instructions(
        SVMStaticMessage::program_instructions_iter(&input.transaction),
        &input.bank_feature_set,
    )?;
    let fee_details = calculate_fee_details(
        &input.transaction,
        input.blockhash_lamports_per_signature,
        compute_budget_limits.get_prioritization_fee(),
        FeeFeatures::from(&input.bank_feature_set),
    );
    let compute_budget_and_limits = compute_budget_limits.get_compute_budget_and_limits(
        compute_budget_limits.loaded_accounts_bytes,
        fee_details,
        runtime_features.raise_cpi_nesting_limit_to_8,
    );
    Ok(crate::account_loader::CheckedTransactionDetails::new(
        input.transaction.get_durable_nonce().copied(),
        compute_budget_and_limits,
    ))
}

fn account_shared_data_to_account(
    (pubkey, account): &(Pubkey, AccountSharedData),
) -> (Pubkey, solana_account::Account) {
    (*pubkey, account.clone().into())
}

fn rollback_accounts_to_accounts(
    rollback_accounts: &crate::rollback_accounts::RollbackAccounts,
) -> Vec<(Pubkey, solana_account::Account)> {
    rollback_accounts
        .iter()
        .map(account_shared_data_to_account)
        .collect()
}

fn effects_from_processing_result(
    result: TransactionProcessingResult,
    transaction: &SanitizedTransaction,
) -> TxnEffects {
    match result {
        Ok(ProcessedTransaction::Executed(executed_tx)) => {
            let status = executed_tx.execution_details.status.clone();
            let modified_accounts = executed_tx
                .loaded_transaction
                .accounts
                .iter()
                .enumerate()
                .filter(|(index, _)| transaction.is_writable(*index))
                .map(|(_, account)| account_shared_data_to_account(account))
                .collect();
            let rollback_accounts = if status.is_err() {
                rollback_accounts_to_accounts(&executed_tx.loaded_transaction.rollback_accounts)
            } else {
                vec![]
            };
            let return_data = executed_tx
                .execution_details
                .return_data
                .as_ref()
                .map(|return_data| return_data.data.clone())
                .unwrap_or_default();
            let compute_unit_limit = executed_tx
                .loaded_transaction
                .compute_budget
                .compute_unit_limit;
            let executed_units = executed_tx.execution_details.executed_units;

            TxnEffects {
                executed: true,
                status,
                modified_accounts,
                rollback_accounts,
                return_data,
                executed_units,
                fee_details: executed_tx.loaded_transaction.fee_details,
                loaded_accounts_data_size: u64::from(
                    executed_tx.loaded_transaction.loaded_accounts_data_size,
                ),
                cu_avail: compute_unit_limit.saturating_sub(executed_units),
            }
        }
        Ok(ProcessedTransaction::FeesOnly(tx)) => TxnEffects {
            executed: true,
            status: Err(tx.load_error.clone()),
            modified_accounts: vec![],
            rollback_accounts: rollback_accounts_to_accounts(&tx.rollback_accounts),
            return_data: vec![],
            executed_units: 0,
            fee_details: tx.fee_details,
            loaded_accounts_data_size: u64::from(tx.loaded_accounts_data_size),
            cu_avail: 0,
        },
        Err(err) => TxnEffects {
            executed: false,
            status: Err(err),
            modified_accounts: vec![],
            rollback_accounts: vec![],
            return_data: vec![],
            executed_units: 0,
            fee_details: FeeDetails::default(),
            loaded_accounts_data_size: 0,
            cu_avail: 0,
        },
    }
}

/// Execute a single transaction against the Solana VM with the default
/// (no-precompile) callback.
pub fn execute_txn(input: &TxnContext) -> TxnEffects {
    execute_txn_with_callback(input, &DefaultCallback)
}

/// Execute a single transaction with a custom callback.
pub fn execute_txn_with_callback<C: InvokeContextCallback>(
    input: &TxnContext,
    callback: &C,
) -> TxnEffects {
    let runtime_features = input.bank_feature_set.runtime_features();
    let compute_budget =
        ComputeBudget::new_with_defaults(runtime_features.raise_cpi_nesting_limit_to_8);
    let runtime_environments = program_runtime_environments(&runtime_features, &compute_budget);
    let fork_graph = Arc::new(RwLock::new(TxnForkGraph));
    let transaction_processor = TransactionBatchProcessor::new(
        input.slot,
        input.epoch,
        Arc::downgrade(&fork_graph),
        Some(runtime_environments.get_env_for_execution().clone()),
    );
    add_builtins_to_transaction_batch_processor(&transaction_processor);

    let callback = TxnCallback {
        input,
        invoke_callback: callback,
    };
    transaction_processor.fill_missing_sysvar_cache_entries(&callback);

    let environment = TransactionProcessingEnvironment {
        blockhash: input.blockhash,
        blockhash_lamports_per_signature: input.blockhash_lamports_per_signature,
        alpenglow_migration_succeeded: false,
        epoch_total_stake: input.epoch_total_stake,
        feature_set: runtime_features,
        program_runtime_environments: runtime_environments,
        rent: input.rent.clone(),
    };
    let config = TransactionProcessingConfig {
        recording_config: ExecutionRecordingConfig {
            enable_cpi_recording: false,
            enable_log_recording: true,
            enable_return_data_recording: true,
            enable_transaction_balance_recording: false,
        },
        drop_on_failure: input.drop_on_failure,
        limit_to_load_programs: true,
        ..Default::default()
    };
    let check_result = transaction_check_result(input);
    let result = transaction_processor
        .load_and_execute_sanitized_transactions(
            &callback,
            std::slice::from_ref(&input.transaction),
            vec![check_result],
            &environment,
            &config,
        )
        .processing_results
        .into_iter()
        .next()
        .unwrap_or(Err(TransactionError::SanitizeFailure));

    effects_from_processing_result(result, &input.transaction)
}

#[cfg(feature = "conformance")]
pub fn execute_txn_proto(input: ProtoTxnContext) -> ProtoTxnResult {
    let context = TxnContext::from(input);

    let virtual_address_space_adjustments_active = context
        .bank_feature_set
        .runtime_features()
        .virtual_address_space_adjustments;
    let mut effects = execute_txn_with_callback(&context, &ConformanceCallback);

    direct_mapping_handle_cu_exhaustion(
        virtual_address_space_adjustments_active,
        effects.cu_avail,
        effects.status.is_err(),
        effects
            .modified_accounts
            .iter_mut()
            .map(|(_, account)| &mut account.data),
    );

    effects.into()
}

/// Due to how Firedancer's VM CU accounting works, when
/// `virtual_address_space_adjustments` is enabled and execution fails with the
/// CU meter exhausted, we cannot compare the data region of the accounts with
/// Agave. Clears each supplied data buffer in that case.
pub fn direct_mapping_handle_cu_exhaustion<'a>(
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

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::conformance::programs::keyed_account_for_system_program,
        solana_account::Account,
        solana_keypair::Keypair,
        solana_message::Message,
        solana_rent::Rent,
        solana_signer::Signer,
        solana_system_interface::instruction as system_instruction,
        solana_transaction::{Transaction, sanitized::SanitizedTransaction},
    };

    const LAMPORTS_PER_SIGNATURE: u64 = 5_000;

    fn system_account_with_lamports(lamports: u64) -> Account {
        Account {
            lamports,
            data: vec![],
            owner: solana_sdk_ids::system_program::id(),
            executable: false,
            rent_epoch: u64::MAX,
        }
    }

    #[test]
    fn test_system_transfer() {
        let from = Keypair::new();
        let to = Pubkey::new_unique();
        let recent_blockhash = solana_hash::Hash::new_unique();
        let amount = 10;
        let transaction = SanitizedTransaction::from_transaction_for_tests(Transaction::new(
            &[&from],
            Message::new(
                &[system_instruction::transfer(&from.pubkey(), &to, amount)],
                Some(&from.pubkey()),
            ),
            recent_blockhash,
        ));
        let context = TxnContext {
            bank_feature_set: agave_feature_set::FeatureSet::default(),
            accounts: vec![
                (from.pubkey(), system_account_with_lamports(10_000_000)),
                (to, system_account_with_lamports(1_000_000)),
                keyed_account_for_system_program(),
            ],
            transaction,
            slot: 0,
            epoch: 0,
            blockhash: recent_blockhash,
            blockhash_lamports_per_signature: LAMPORTS_PER_SIGNATURE,
            epoch_total_stake: 0,
            rent: Rent::default(),
            drop_on_failure: false,
        };

        let effects = execute_txn(&context);

        assert_eq!(effects.status, Ok(()));
        assert!(effects.executed);
        assert_eq!(
            effects.fee_details.transaction_fee(),
            LAMPORTS_PER_SIGNATURE
        );
        assert_eq!(
            effects.get_account(&from.pubkey()).unwrap().lamports,
            10_000_000 - LAMPORTS_PER_SIGNATURE - amount
        );
        assert_eq!(
            effects.get_account(&to).unwrap().lamports,
            1_000_000 + amount
        );
    }
}
