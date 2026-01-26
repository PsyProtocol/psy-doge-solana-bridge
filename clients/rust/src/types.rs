//! Type re-exports and new types for the bridge client.
//!
//! This module consolidates type exports from various crates and defines
//! new types specific to the client API.

// Re-exports from psy-bridge-core
pub use psy_bridge_core::{
    crypto::zk::CompactBridgeZKProof,
    header::{PsyBridgeHeader, PsyBridgeHeaderUpdate, PsyBridgeStateCommitment, PsyBridgeTipStateCommitment},
};

// Re-exports from psy-doge-solana-core
pub use psy_doge_solana_core::{
    data_accounts::pending_mint::PendingMint,
    instructions::doge_bridge::InitializeBridgeParams,
    program_state::{
        FinalizedBlockMintTxoInfo, PsyBridgeConfig, PsyBridgeProgramState,
        PsyReturnTxOutput, PsyWithdrawalChainSnapshot, PsyWithdrawalRequest,
    },
};

use solana_sdk::signature::Signature;

/// Record for a deposit transaction output.
///
/// Used when querying manual deposits from the bridge state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepositTxOutputRecord {
    /// Transaction hash (Dogecoin tx hash)
    pub tx_hash: [u8; 32],
    /// Combined TXO index (encodes block, tx, output)
    pub combined_txo_index: u64,
    /// Recipient's Solana public key
    pub recipient_pubkey: [u8; 32],
    /// Deposit amount in satoshis
    pub amount_sats: u64,
    /// Block height where deposit was included
    pub block_height: u32,
}

/// Result of processing pending mint groups.
#[derive(Debug, Clone)]
pub struct ProcessMintsResult {
    /// Number of groups processed
    pub groups_processed: usize,
    /// Total number of mints processed
    pub total_mints_processed: usize,
    /// Transaction signatures for each processed group
    pub signatures: Vec<Signature>,
    /// Whether all pending mints were processed
    pub fully_completed: bool,
}

impl ProcessMintsResult {
    /// Create a new result indicating no mints were processed.
    pub fn empty() -> Self {
        Self {
            groups_processed: 0,
            total_mints_processed: 0,
            signatures: vec![],
            fully_completed: true,
        }
    }

    /// Create a new result with the given values.
    pub fn new(
        groups_processed: usize,
        total_mints_processed: usize,
        signatures: Vec<Signature>,
        fully_completed: bool,
    ) -> Self {
        Self {
            groups_processed,
            total_mints_processed,
            signatures,
            fully_completed,
        }
    }
}

/// Bridge state with additional derived information.
#[derive(Debug, Clone)]
pub struct BridgeStateInfo {
    /// Core bridge program state
    pub state: PsyBridgeProgramState,
    /// DOGE token mint address
    pub doge_mint: solana_sdk::pubkey::Pubkey,
    /// Bridge state PDA
    pub bridge_state_pda: solana_sdk::pubkey::Pubkey,
}

/// Withdrawal request with tracking information.
#[derive(Debug, Clone)]
pub struct WithdrawalRequestInfo {
    /// The withdrawal request details
    pub request: PsyWithdrawalRequest,
    /// Index in the withdrawal queue
    pub index: u64,
    /// User's Solana public key
    pub user_pubkey: [u8; 32],
}

/// Transaction result with confirmation details.
#[derive(Debug, Clone)]
pub struct TransactionResult {
    /// Transaction signature
    pub signature: Signature,
    /// Slot where transaction was confirmed
    pub slot: u64,
}

impl TransactionResult {
    /// Create a new transaction result.
    pub fn new(signature: Signature, slot: u64) -> Self {
        Self { signature, slot }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_mints_result_empty() {
        let result = ProcessMintsResult::empty();
        assert_eq!(result.groups_processed, 0);
        assert_eq!(result.total_mints_processed, 0);
        assert!(result.fully_completed);
    }

    #[test]
    fn test_deposit_record_creation() {
        let record = DepositTxOutputRecord {
            tx_hash: [1u8; 32],
            combined_txo_index: 12345,
            recipient_pubkey: [2u8; 32],
            amount_sats: 1_000_000,
            block_height: 100,
        };

        assert_eq!(record.amount_sats, 1_000_000);
        assert_eq!(record.block_height, 100);
    }
}
