//! API traits for the bridge client.
//!
//! This module defines the public API traits that the BridgeClient implements.
//! These traits provide a clean abstraction for all bridge operations.

pub mod blocks;
pub mod buffers;
pub mod deposits;
pub mod mints;
pub mod state;
pub mod withdrawals;

use async_trait::async_trait;
use solana_sdk::{pubkey::Pubkey, signature::{Keypair, Signature}};

use crate::{
    errors::BridgeError,
    types::{
        CompactBridgeZKProof, DepositTxOutputRecord, FinalizedBlockMintTxoInfo,
        InitializeBridgeParams, PendingMint, ProcessMintsResult, PsyBridgeHeader,
        PsyBridgeProgramState, PsyReturnTxOutput, PsyWithdrawalChainSnapshot,
    },
};

/// Main API trait for bridge operations.
///
/// Provides methods for querying bridge state, processing blocks,
/// handling mints, and managing buffers.
#[async_trait]
pub trait BridgeApi: Send + Sync {
    /// Get the current bridge program state from on-chain.
    async fn get_current_bridge_state(&self) -> Result<PsyBridgeProgramState, BridgeError>;

    /// Get manual deposits starting from a specific index.
    ///
    /// Returns up to `max_count` deposit records starting from the given index.
    async fn get_manual_deposits_at(
        &self,
        next_processed_manual_deposit_index: u64,
        max_count: u32,
    ) -> Result<Vec<DepositTxOutputRecord>, BridgeError>;

    /// Process remaining pending mint groups.
    ///
    /// Processes all unclaimed mint groups using the standard mint instruction.
    async fn process_remaining_pending_mints_groups(
        &self,
        pending_mints: &[PendingMint],
        mint_buffer_account: Pubkey,
        mint_buffer_bump: u8,
    ) -> Result<ProcessMintsResult, BridgeError>;

    /// Process remaining pending mint groups with auto-advance.
    ///
    /// Uses the auto-advance instruction that also updates the TXO buffer.
    async fn process_remaining_pending_mints_groups_auto_advance(
        &self,
        pending_mints: &[PendingMint],
        mint_buffer_account: Pubkey,
        mint_buffer_bump: u8,
        txo_buffer_account: Pubkey,
        txo_buffer_bump: u8,
    ) -> Result<ProcessMintsResult, BridgeError>;

    /// Process a block transition.
    ///
    /// Submits a new block to the bridge with ZK proof verification.
    async fn process_block_transition(
        &self,
        proof: CompactBridgeZKProof,
        header: PsyBridgeHeader,
        mint_buffer_account: Pubkey,
        mint_buffer_bump: u8,
        txo_buffer_account: Pubkey,
        txo_buffer_bump: u8,
    ) -> Result<Signature, BridgeError>;

    /// Process a block reorganization.
    ///
    /// Handles chain reorgs by submitting multiple blocks at once.
    async fn process_block_reorg(
        &self,
        proof: CompactBridgeZKProof,
        header: PsyBridgeHeader,
        extra_blocks: Vec<FinalizedBlockMintTxoInfo>,
        mint_buffer_account: Pubkey,
        mint_buffer_bump: u8,
        txo_buffer_account: Pubkey,
        txo_buffer_bump: u8,
    ) -> Result<Signature, BridgeError>;

    /// Setup a TXO buffer for a block.
    ///
    /// Creates and populates a TXO buffer with the given indices.
    /// Returns the buffer address and bump seed.
    async fn setup_txo_buffer(
        &self,
        block_height: u32,
        txos: &[u32],
    ) -> Result<(Pubkey, u8), BridgeError>;

    /// Setup a pending mints buffer.
    ///
    /// Creates and populates a pending mints buffer with the given mints.
    /// Returns the buffer address and bump seed.
    async fn setup_pending_mints_buffer(
        &self,
        block_height: u32,
        pending_mints: &[PendingMint],
    ) -> Result<(Pubkey, u8), BridgeError>;

    /// Get the current withdrawal chain snapshot.
    async fn snapshot_withdrawals(&self) -> Result<PsyWithdrawalChainSnapshot, BridgeError>;
}

/// API trait for withdrawal operations.
#[async_trait]
pub trait WithdrawalApi: Send + Sync {
    /// Request a withdrawal from Solana to Dogecoin.
    ///
    /// Burns tokens on Solana to receive DOGE on the Dogecoin network.
    async fn request_withdrawal(
        &self,
        user_authority: &Keypair,
        recipient_address: [u8; 20],
        amount_sats: u64,
        address_type: u32,
    ) -> Result<Signature, BridgeError>;

    /// Process a withdrawal transaction.
    ///
    /// Submits the Dogecoin transaction that fulfills withdrawals.
    async fn process_withdrawal(
        &self,
        proof: CompactBridgeZKProof,
        new_return_output: PsyReturnTxOutput,
        new_spent_txo_tree_root: [u8; 32],
        new_next_processed_withdrawals_index: u64,
        doge_tx_bytes: &[u8],
    ) -> Result<Signature, BridgeError>;

    /// Replay a withdrawal message (for Wormhole integration).
    async fn replay_withdrawal(&self, doge_tx_bytes: &[u8]) -> Result<Signature, BridgeError>;
}

/// API trait for manual claim operations.
#[async_trait]
pub trait ManualClaimApi: Send + Sync {
    /// Execute a manual claim for a deposit.
    ///
    /// Claims a deposit that was not auto-claimed during block processing.
    async fn manual_claim_deposit(
        &self,
        user_signer: &Keypair,
        proof: CompactBridgeZKProof,
        recent_block_merkle_tree_root: [u8; 32],
        recent_auto_claim_txo_root: [u8; 32],
        new_manual_claim_txo_root: [u8; 32],
        tx_hash: [u8; 32],
        combined_txo_index: u64,
        deposit_amount_sats: u64,
    ) -> Result<Signature, BridgeError>;
}

/// API trait for operator-only operations.
#[async_trait]
pub trait OperatorApi: Send + Sync {
    /// Initialize the bridge.
    ///
    /// Can only be called once to set up the bridge state.
    async fn initialize_bridge(
        &self,
        params: &InitializeBridgeParams,
    ) -> Result<Signature, BridgeError>;

    /// Withdraw accumulated fees.
    ///
    /// Operator-only operation to withdraw bridge fees.
    async fn operator_withdraw_fees(&self) -> Result<Signature, BridgeError>;

    /// Execute snapshot withdrawals.
    ///
    /// Operator-only operation to snapshot the current withdrawal chain state.
    async fn execute_snapshot_withdrawals(&self) -> Result<Signature, BridgeError>;
}
