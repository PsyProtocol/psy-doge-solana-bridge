//! Block transition implementations.

use crate::{
    client::BridgeClient,
    errors::BridgeError,
    instructions,
    types::{CompactBridgeZKProof, FinalizedBlockMintTxoInfo, PsyBridgeHeader},
};
use solana_sdk::{pubkey::Pubkey, signature::Signature, signer::Signer};

impl BridgeClient {
    /// Process a block transition.
    pub async fn process_block_transition_impl(
        &self,
        proof: CompactBridgeZKProof,
        header: PsyBridgeHeader,
        mint_buffer_account: Pubkey,
        mint_buffer_bump: u8,
        txo_buffer_account: Pubkey,
        txo_buffer_bump: u8,
    ) -> Result<Signature, BridgeError> {
        let ix = instructions::block_update(
            self.config.program_id,
            self.config.payer.pubkey(),
            proof,
            header,
            self.config.operator.pubkey(),
            mint_buffer_account,
            txo_buffer_account,
            mint_buffer_bump,
            txo_buffer_bump,
        );

        self.send_and_confirm(&[ix], &[self.config.operator.as_ref()])
            .await
    }

    /// Process a block reorganization.
    pub async fn process_block_reorg_impl(
        &self,
        proof: CompactBridgeZKProof,
        header: PsyBridgeHeader,
        extra_blocks: Vec<FinalizedBlockMintTxoInfo>,
        mint_buffer_account: Pubkey,
        mint_buffer_bump: u8,
        txo_buffer_account: Pubkey,
        txo_buffer_bump: u8,
    ) -> Result<Signature, BridgeError> {
        let ix = instructions::process_reorg_blocks(
            self.config.program_id,
            self.config.payer.pubkey(),
            proof,
            header,
            extra_blocks,
            self.config.operator.pubkey(),
            mint_buffer_account,
            txo_buffer_account,
            mint_buffer_bump,
            txo_buffer_bump,
        );

        self.send_and_confirm(&[ix], &[self.config.operator.as_ref()])
            .await
    }
}
