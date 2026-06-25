//! Transaction effects (output).

#[cfg(feature = "conformance")]
use {
    crate::conformance::account_state::account_to_proto,
    protosol::protos::{FeeDetails as ProtoFeeDetails, TxnResult as ProtoTxnResult},
    solana_instruction::error::InstructionError,
    solana_transaction_error::TransactionError,
};
use {
    solana_account::Account, solana_fee_structure::FeeDetails, solana_pubkey::Pubkey,
    solana_transaction_error::TransactionResult,
};

/// Represents the effects of a single transaction.
pub struct TxnEffects {
    pub executed: bool,
    pub status: TransactionResult<()>,
    pub modified_accounts: Vec<(Pubkey, Account)>,
    pub rollback_accounts: Vec<(Pubkey, Account)>,
    pub return_data: Vec<u8>,
    pub executed_units: u64,
    pub fee_details: FeeDetails,
    pub loaded_accounts_data_size: u64,
    pub cu_avail: u64,
}

impl TxnEffects {
    /// Returns the modified account for the given pubkey, if it exists.
    pub fn get_account(&self, pubkey: &Pubkey) -> Option<&Account> {
        self.modified_accounts
            .iter()
            .find(|(pk, _)| pk == pubkey)
            .map(|(_, acc)| acc)
    }
}

#[cfg(feature = "conformance")]
fn err_num<T: serde::Serialize>(value: &T) -> u32 {
    let serialized = bincode::serialize(value).unwrap_or_else(|_| vec![0; 4]);
    u32::from_le_bytes(serialized[0..4].try_into().unwrap()).saturating_add(1)
}

#[cfg(feature = "conformance")]
fn error_fields(status: &TransactionResult<()>) -> (u32, u32, u32, u32) {
    match status {
        Ok(()) => (0, 0, 0, 0),
        Err(transaction_error) => {
            let (instruction_error, instruction_error_index, custom_error) = match transaction_error
            {
                TransactionError::InstructionError(instruction_error_index, instruction_error) => {
                    let custom_error = match instruction_error {
                        InstructionError::Custom(custom_error) => *custom_error,
                        _ => 0,
                    };
                    (
                        err_num(instruction_error),
                        (*instruction_error_index).into(),
                        custom_error,
                    )
                }
                _ => (0, 0, 0),
            };
            (
                err_num(transaction_error),
                instruction_error,
                instruction_error_index,
                custom_error,
            )
        }
    }
}

#[cfg(feature = "conformance")]
impl From<TxnEffects> for ProtoTxnResult {
    fn from(value: TxnEffects) -> Self {
        let TxnEffects {
            executed,
            status,
            modified_accounts,
            rollback_accounts,
            return_data,
            executed_units,
            fee_details,
            loaded_accounts_data_size,
            ..
        } = value;
        let (txn_error, instruction_error, instruction_error_index, custom_error) =
            error_fields(&status);

        Self {
            executed,
            txn_error,
            instruction_error,
            instruction_error_index,
            custom_error,
            return_data,
            executed_units,
            fee_details: Some(ProtoFeeDetails {
                transaction_fee: fee_details.transaction_fee(),
                prioritization_fee: fee_details.prioritization_fee(),
            }),
            loaded_accounts_data_size,
            modified_accounts: modified_accounts
                .into_iter()
                .map(account_to_proto)
                .collect(),
            rollback_accounts: rollback_accounts
                .into_iter()
                .map(account_to_proto)
                .collect(),
        }
    }
}
