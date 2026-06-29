//! Transaction effects (output).

#[cfg(feature = "conformance")]
use solana_transaction_error::TransactionError;
#[cfg(feature = "conformance")]
use {
    crate::conformance::account_state::account_to_proto,
    protosol::protos::{FeeDetails as ProtoFeeDetails, TxnResult as ProtoTxnResult},
};
use {
    solana_account::AccountSharedData, solana_fee_structure::FeeDetails, solana_pubkey::Pubkey,
    solana_transaction_error::TransactionResult,
};

/// Represents effects of a single transaction.
pub struct TxnEffects {
    pub executed: bool,
    pub status: TransactionResult<()>,
    pub modified_accounts: Vec<(Pubkey, AccountSharedData)>,
    pub rollback_accounts: Vec<(Pubkey, AccountSharedData)>,
    pub return_data: Vec<u8>,
    pub executed_units: u64,
    pub fee_details: FeeDetails,
    pub loaded_accounts_data_size: u64,
    pub logs: Vec<String>,
    pub cu_avail: u64,
}

impl TxnEffects {
    pub fn get_modified_account(&self, pubkey: &Pubkey) -> Option<&AccountSharedData> {
        self.modified_accounts
            .iter()
            .find(|(pk, _)| pk == pubkey)
            .map(|(_, account)| account)
    }
}

#[cfg(feature = "conformance")]
impl From<TxnEffects> for ProtoTxnResult {
    fn from(value: TxnEffects) -> Self {
        let error = ProtoTxnErrorFields::from_transaction_result(&value.status);
        let is_ok = value.status.is_ok();
        let sanitization_error = matches!(value.status, Err(TransactionError::SanitizeFailure));
        Self {
            executed: value.executed,
            sanitization_error,
            is_ok,
            status: error.txn_error,
            instruction_error: error.instruction_error,
            instruction_error_index: error.instruction_error_index,
            custom_error: error.custom_error,
            return_data: value.return_data,
            executed_units: value.executed_units,
            fee_details: Some(ProtoFeeDetails {
                transaction_fee: value.fee_details.transaction_fee(),
                prioritization_fee: value.fee_details.prioritization_fee(),
            }),
            loaded_accounts_data_size: value.loaded_accounts_data_size,
            modified_accounts: value
                .modified_accounts
                .into_iter()
                .map(|(pubkey, account)| account_to_proto((pubkey, account.into())))
                .collect(),
            rollback_accounts: value
                .rollback_accounts
                .into_iter()
                .map(|(pubkey, account)| account_to_proto((pubkey, account.into())))
                .collect(),
        }
    }
}

#[cfg(feature = "conformance")]
#[derive(Default)]
pub(crate) struct ProtoTxnErrorFields {
    pub(crate) txn_error: u32,
    pub(crate) instruction_error: u32,
    pub(crate) custom_error: u32,
    pub(crate) instruction_error_index: u32,
}

#[cfg(feature = "conformance")]
impl ProtoTxnErrorFields {
    pub(crate) fn from_transaction_result(status: &TransactionResult<()>) -> Self {
        status
            .as_ref()
            .err()
            .map(Self::from_transaction_error)
            .unwrap_or_default()
    }

    pub(crate) fn from_transaction_error(transaction_error: &TransactionError) -> Self {
        fn err_num<T: serde::Serialize>(value: &T) -> u32 {
            let serialized = bincode::serialize(value).unwrap_or_else(|_| vec![0; 4]);
            u32::from_le_bytes(serialized[0..4].try_into().unwrap()).saturating_add(1)
        }

        let (instruction_error, custom_error, instruction_error_index) = match transaction_error {
            TransactionError::InstructionError(instruction_error_index, instruction_error) => {
                let custom_error = match instruction_error {
                    solana_instruction::error::InstructionError::Custom(custom_error) => {
                        *custom_error
                    }
                    _ => 0,
                };
                (
                    err_num(instruction_error),
                    custom_error,
                    (*instruction_error_index).into(),
                )
            }
            _ => (0, 0, 0),
        };

        Self {
            txn_error: err_num(transaction_error),
            instruction_error,
            custom_error,
            instruction_error_index,
        }
    }
}
