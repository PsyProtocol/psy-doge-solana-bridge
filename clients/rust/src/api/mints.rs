//! Mint processing implementations.

use crate::{
    client::BridgeClient,
    errors::BridgeError,
    instructions,
    types::{PendingMint, ProcessMintsResult},
};
use psy_doge_solana_core::data_accounts::pending_mint::PM_MAX_PENDING_MINTS_PER_GROUP;
use solana_sdk::{pubkey::Pubkey, signer::Signer};

impl BridgeClient {
    /// Process remaining pending mint groups.
    pub async fn process_remaining_pending_mints_groups_impl(
        &self,
        pending_mints: &[PendingMint],
        mint_buffer_account: Pubkey,
        mint_buffer_bump: u8,
    ) -> Result<ProcessMintsResult, BridgeError> {
        if pending_mints.is_empty() {
            return Ok(ProcessMintsResult::empty());
        }

        let state = self.get_current_bridge_state_impl().await?;
        let doge_mint = self.get_doge_mint().await?;

        let tracker = &state.pending_mint_txos.current_pending_mints_tracker;
        let total_groups = tracker.get_current_total_pending_mints_groups();

        let mut signatures = Vec::new();
        let mut groups_processed = 0;
        let mut total_mints_processed = 0;

        for group_idx in 0..total_groups {
            // Skip already claimed groups
            if tracker.is_pending_mints_group_already_claimed(group_idx) {
                continue;
            }

            let is_last = group_idx == total_groups - 1;
            let group_start = group_idx as usize * PM_MAX_PENDING_MINTS_PER_GROUP;
            let group_end = std::cmp::min(
                group_start + PM_MAX_PENDING_MINTS_PER_GROUP,
                pending_mints.len(),
            );
            let group_mints = &pending_mints[group_start..group_end];

            let recipients: Vec<Pubkey> = group_mints
                .iter()
                .map(|m| Pubkey::new_from_array(m.recipient))
                .collect();

            let ix = instructions::process_mint_group(
                self.config.program_id,
                self.config.operator.pubkey(),
                mint_buffer_account,
                doge_mint,
                recipients,
                group_idx,
                mint_buffer_bump,
                is_last,
            );

            let sig = self
                .send_and_confirm(&[ix], &[self.config.operator.as_ref()])
                .await?;

            signatures.push(sig);
            groups_processed += 1;
            total_mints_processed += group_mints.len();
        }

        Ok(ProcessMintsResult::new(
            groups_processed,
            total_mints_processed,
            signatures,
            true,
        ))
    }

    /// Process remaining pending mint groups with auto-advance.
    pub async fn process_remaining_pending_mints_groups_auto_advance_impl(
        &self,
        pending_mints: &[PendingMint],
        mint_buffer_account: Pubkey,
        mint_buffer_bump: u8,
        txo_buffer_account: Pubkey,
        txo_buffer_bump: u8,
    ) -> Result<ProcessMintsResult, BridgeError> {
        if pending_mints.is_empty() {
            return Ok(ProcessMintsResult::empty());
        }

        let state = self.get_current_bridge_state_impl().await?;
        let doge_mint = self.get_doge_mint().await?;

        let tracker = &state.pending_mint_txos.current_pending_mints_tracker;
        let total_groups = tracker.get_current_total_pending_mints_groups();

        let mut signatures = Vec::new();
        let mut groups_processed = 0;
        let mut total_mints_processed = 0;

        for group_idx in 0..total_groups {
            // Skip already claimed groups
            if tracker.is_pending_mints_group_already_claimed(group_idx) {
                continue;
            }

            let is_last = group_idx == total_groups - 1;
            let group_start = group_idx as usize * PM_MAX_PENDING_MINTS_PER_GROUP;
            let group_end = std::cmp::min(
                group_start + PM_MAX_PENDING_MINTS_PER_GROUP,
                pending_mints.len(),
            );
            let group_mints = &pending_mints[group_start..group_end];

            let recipients: Vec<Pubkey> = group_mints
                .iter()
                .map(|m| Pubkey::new_from_array(m.recipient))
                .collect();

            let ix = instructions::process_mint_group_auto_advance(
                self.config.program_id,
                self.config.operator.pubkey(),
                mint_buffer_account,
                txo_buffer_account,
                doge_mint,
                recipients,
                group_idx,
                mint_buffer_bump,
                txo_buffer_bump,
                is_last,
            );

            let sig = self
                .send_and_confirm(&[ix], &[self.config.operator.as_ref()])
                .await?;

            signatures.push(sig);
            groups_processed += 1;
            total_mints_processed += group_mints.len();
        }

        Ok(ProcessMintsResult::new(
            groups_processed,
            total_mints_processed,
            signatures,
            true,
        ))
    }
}
