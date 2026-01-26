//! Deposit and manual claim implementations.

use crate::{
    client::BridgeClient,
    errors::BridgeError,
    instructions,
    types::{CompactBridgeZKProof, DepositTxOutputRecord, InitializeBridgeParams},
};
use solana_sdk::{signature::{Keypair, Signature}, signer::Signer};

impl BridgeClient {
    /// Get manual deposits starting from a specific index.
    ///
    /// Note: This is a placeholder implementation. The actual implementation
    /// would need to query the manual_deposits_tree from the bridge state.
    pub async fn get_manual_deposits_at_impl(
        &self,
        _next_processed_manual_deposit_index: u64,
        _max_count: u32,
    ) -> Result<Vec<DepositTxOutputRecord>, BridgeError> {
        // The manual deposits are stored in a Merkle tree on-chain.
        // To retrieve them, we would need to either:
        // 1. Index events from transaction logs
        // 2. Use a separate indexer service
        // 3. Store deposit data in a separate account
        //
        // For now, return empty vec as this requires off-chain indexing
        Ok(vec![])
    }

    /// Execute a manual claim for a deposit.
    pub async fn manual_claim_deposit_impl(
        &self,
        user_signer: &Keypair,
        proof: CompactBridgeZKProof,
        recent_block_merkle_tree_root: [u8; 32],
        recent_auto_claim_txo_root: [u8; 32],
        new_manual_claim_txo_root: [u8; 32],
        tx_hash: [u8; 32],
        combined_txo_index: u64,
        deposit_amount_sats: u64,
    ) -> Result<Signature, BridgeError> {
        let doge_mint = self.get_doge_mint().await?;

        let user_token_account = spl_associated_token_account::get_associated_token_address(
            &user_signer.pubkey(),
            &doge_mint,
        );

        let ix = instructions::manual_claim_deposit_instruction(
            self.config.manual_claim_program_id,
            self.config.program_id,
            user_signer.pubkey(),
            self.config.payer.pubkey(),
            doge_mint,
            user_token_account,
            proof,
            recent_block_merkle_tree_root,
            recent_auto_claim_txo_root,
            new_manual_claim_txo_root,
            tx_hash,
            combined_txo_index,
            deposit_amount_sats,
        );

        self.send_and_confirm(&[ix], &[user_signer]).await
    }

    /// Initialize the bridge.
    pub async fn initialize_bridge_impl(
        &self,
        params: &InitializeBridgeParams,
    ) -> Result<Signature, BridgeError> {
        let doge_mint = self
            .config
            .doge_mint
            .ok_or_else(|| BridgeError::MissingField {
                field: "doge_mint".to_string(),
            })?;

        let ix = instructions::initialize_bridge(
            self.config.payer.pubkey(),
            self.config.operator.pubkey(),
            self.config.payer.pubkey(), // fee_spender = payer for now
            doge_mint,
            params,
        );

        self.send_and_confirm(&[ix], &[self.config.operator.as_ref()])
            .await
    }

    /// Withdraw accumulated fees (operator only).
    pub async fn operator_withdraw_fees_impl(&self) -> Result<Signature, BridgeError> {
        let doge_mint = self.get_doge_mint().await?;

        let operator_token_account = spl_associated_token_account::get_associated_token_address(
            &self.config.operator.pubkey(),
            &doge_mint,
        );

        let ix = instructions::operator_withdraw_fees(
            self.config.program_id,
            self.config.operator.pubkey(),
            operator_token_account,
            doge_mint,
        );

        self.send_and_confirm(&[ix], &[self.config.operator.as_ref()])
            .await
    }
}
