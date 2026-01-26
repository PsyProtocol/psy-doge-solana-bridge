//! Buffer setup implementations.

use crate::{client::BridgeClient, errors::BridgeError, types::PendingMint};
use solana_sdk::pubkey::Pubkey;

impl BridgeClient {
    /// Setup a TXO buffer for a block.
    pub async fn setup_txo_buffer_impl(
        &self,
        block_height: u32,
        txos: &[u32],
    ) -> Result<(Pubkey, u8), BridgeError> {
        self.buffer_manager
            .create_txo_buffer(self.config.txo_buffer_program_id, block_height, txos)
            .await
    }

    /// Setup a pending mints buffer.
    pub async fn setup_pending_mints_buffer_impl(
        &self,
        _block_height: u32,
        pending_mints: &[PendingMint],
    ) -> Result<(Pubkey, u8), BridgeError> {
        self.buffer_manager
            .create_pending_mint_buffer(
                self.config.pending_mint_program_id,
                self.config.bridge_state_pda,
                pending_mints,
            )
            .await
    }
}
