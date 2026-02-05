//! Withdrawal processing implementations.

use crate::{
    client::BridgeClient,
    errors::BridgeError,
    instructions,
    types::{CompactBridgeZKProof, PsyReturnTxOutput},
};
use solana_sdk::{signature::{Keypair, Signature}, signer::Signer};

impl BridgeClient {
    /// Request a withdrawal from Solana to Dogecoin.
    pub async fn request_withdrawal_impl(
        &self,
        user_authority: &Keypair,
        recipient_address: [u8; 20],
        amount_sats: u64,
        address_type: u32,
    ) -> Result<Signature, BridgeError> {
        let doge_mint = self.get_doge_mint().await?;

        let user_token_account = spl_associated_token_account::get_associated_token_address(
            &user_authority.pubkey(),
            &doge_mint,
        );

        let ix = instructions::request_withdrawal(
            self.config.program_id,
            user_authority.pubkey(),
            doge_mint,
            user_token_account,
            recipient_address,
            amount_sats,
            address_type,
        );

        self.send_and_confirm(&[ix], &[user_authority]).await
    }

    /// Process a withdrawal transaction.
    pub async fn process_withdrawal_impl(
        &self,
        proof: CompactBridgeZKProof,
        new_return_output: PsyReturnTxOutput,
        new_spent_txo_tree_root: [u8; 32],
        new_next_processed_withdrawals_index: u64,
        new_total_spent_deposit_utxo_count: u64,
        doge_tx_bytes: &[u8],
    ) -> Result<Signature, BridgeError> {
        // Create generic buffer for tx data
        let buffer = self
            .buffer_manager
            .create_generic_buffer(self.config.generic_buffer_program_id, doge_tx_bytes)
            .await?;

        let ix = instructions::process_withdrawal(
            self.config.program_id,
            self.config.payer.pubkey(),
            buffer,
            self.config.wormhole_shim_program_id,
            self.config.wormhole_core_program_id,
            proof,
            new_return_output,
            new_spent_txo_tree_root,
            new_next_processed_withdrawals_index,
            new_total_spent_deposit_utxo_count,
        );

        self.send_and_confirm(&[ix], &[self.config.operator.as_ref()])
            .await
    }

    /// Replay a withdrawal message.
    pub async fn replay_withdrawal_impl(
        &self,
        doge_tx_bytes: &[u8],
    ) -> Result<Signature, BridgeError> {
        // Create generic buffer for tx data
        let buffer = self
            .buffer_manager
            .create_generic_buffer(self.config.generic_buffer_program_id, doge_tx_bytes)
            .await?;

        let ix = instructions::process_replay_withdrawal(
            self.config.program_id,
            self.config.payer.pubkey(),
            buffer,
            self.config.wormhole_shim_program_id,
            self.config.wormhole_core_program_id,
        );

        self.send_and_confirm(&[ix], &[]).await
    }

    /// Execute the snapshot withdrawals instruction on-chain.
    pub async fn execute_snapshot_withdrawals_impl(&self) -> Result<Signature, BridgeError> {
        let ix = instructions::snapshot_withdrawals(
            self.config.program_id,
            self.config.operator.pubkey(),
            self.config.payer.pubkey(),
        );

        self.send_and_confirm(&[ix], &[self.config.operator.as_ref()]).await
    }
}
