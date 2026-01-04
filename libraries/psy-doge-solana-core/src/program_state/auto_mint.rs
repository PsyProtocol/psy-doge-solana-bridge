use psy_bridge_core::{
    crypto::hash::sha256_impl::hash_impl_sha256_bytes,
    error::{DogeBridgeError, QDogeResult},
};

use crate::{
    data_accounts::pending_mint::{
        PM_DA_PENDING_MINT_SIZE, PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE, PM_MAX_PENDING_MINTS_PER_GROUP_U16, PM_TXO_BUFFER_HEADER_SIZE, PendingMint, PendingMintsBufferStateHeader, PendingMintsTxoBufferHeader, pm_txo_data_account_min_size
    },
    generic_cpi::{MintCPIHelper, UnlockAutoClaimMintBufferCPIHelper},
    program_state::{PendingMintsTracker, PsyBridgeProgramState, compute_mint_group_info},
};

pub fn get_nth_pending_mint_offset(pending_mint_groups_count: u16, global_mint_idx: u16) -> usize {
    PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE
        + (pending_mint_groups_count as usize * 32)
        + (global_mint_idx as usize * PM_DA_PENDING_MINT_SIZE)
}

impl PsyBridgeProgramState {
    pub fn run_auto_mint_group_precheck(
        &mut self,
        mint_group_index: u16,
        pending_mints_buffer_pubkey: &[u8; 32],
    ) -> QDogeResult<(bool, u16, usize)> {
        if self.pending_mint_txos.is_empty() {
            return Err(DogeBridgeError::NoPendingMintsToAutoProcess);
        }
        let total_current_pending_mints_groups = self
            .pending_mint_txos
            .current_pending_mints_tracker
            .get_current_total_pending_mints_groups();

        if total_current_pending_mints_groups == 0 {
            return Err(DogeBridgeError::NoPendingMintsToAutoProcess);
        }
        if self
            .pending_mint_txos
            .current_pending_mints_tracker
            .is_empty()
        {
            return Err(DogeBridgeError::NoPendingMintsToAutoProcess);
        }
        self.pending_mint_txos
            .current_pending_mints_tracker
            .ensure_can_claim_pending_mints_group(mint_group_index)?;

        let mints_count_for_current_group =
            if mint_group_index == total_current_pending_mints_groups - 1 {
                let rem = self
                    .pending_mint_txos
                    .current_pending_mints_tracker
                    .total_pending_mints as u16
                    % PM_MAX_PENDING_MINTS_PER_GROUP_U16;
                if rem == 0 {
                    PM_MAX_PENDING_MINTS_PER_GROUP_U16
                } else {
                    rem
                }
            } else {
                PM_MAX_PENDING_MINTS_PER_GROUP_U16
            };

        let pending_mints_buffer_offset_start = get_nth_pending_mint_offset(
            total_current_pending_mints_groups,
            mint_group_index * PM_MAX_PENDING_MINTS_PER_GROUP_U16,
        );

        let buffer_account = &self.pending_mint_txos.current_pending_mints_tracker.last_finalized_auto_claim_mints_storage_account;
        if pending_mints_buffer_pubkey != buffer_account {
            return Err(DogeBridgeError::InvalidAccountKey);
        }
        let can_unlock = self.pending_mint_txos
            .mark_pending_mints_group_claimed(mint_group_index)?;

        

        Ok((can_unlock, mints_count_for_current_group, pending_mints_buffer_offset_start))
    }
    pub fn run_auto_mint_group<Minter: MintCPIHelper>(
        &mut self,
        minter: &Minter,
        self_bridge_program_pub_key: &[u8; 32],
        mint_buffer_public_key: &[u8; 32],
        // get auto_claim_mint_buffer_data_account_memory from the self.pending_mint_txos.current_pending_mints_tracker.last_finalized_auto_claim_mints_storage_account
        auto_claim_mint_buffer_data_account_memory: &[u8],
        mint_group_index: u16,
    ) -> QDogeResult<(bool, usize)> {
        if self.pending_mint_txos.is_empty() {
            return Err(DogeBridgeError::NoPendingMintsToAutoProcess);
        }
        let total_current_pending_mints_groups = self
            .pending_mint_txos
            .current_pending_mints_tracker
            .get_current_total_pending_mints_groups();

        if total_current_pending_mints_groups == 0 {
            return Err(DogeBridgeError::NoPendingMintsToAutoProcess);
        }
        if self
            .pending_mint_txos
            .current_pending_mints_tracker
            .is_empty()
        {
            return Err(DogeBridgeError::NoPendingMintsToAutoProcess);
        }
        self.pending_mint_txos
            .current_pending_mints_tracker
            .ensure_can_claim_pending_mints_group(mint_group_index)?;

        let auto_claim_mints_header: &PendingMintsBufferStateHeader = bytemuck::from_bytes(
            &auto_claim_mint_buffer_data_account_memory
                [0..PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE],
        );

        if mint_group_index >= auto_claim_mints_header.pending_mint_groups_count {
            // sanity check
            return Err(DogeBridgeError::PendingMintsGroupIndexOutOfBounds);
        }
        if &auto_claim_mints_header.authorized_locker_public_key != self_bridge_program_pub_key {
            // even though we check this before setting auto_claim_mints_header.authorized_locker_public_key, we do a snaity check
            return Err(DogeBridgeError::InvalidMintBufferLockingPermission);
        }
        let mints_count_for_current_group =
            if mint_group_index == total_current_pending_mints_groups - 1 {
                let rem = self
                    .pending_mint_txos
                    .current_pending_mints_tracker
                    .total_pending_mints as u16
                    % PM_MAX_PENDING_MINTS_PER_GROUP_U16;
                if rem == 0 {
                    PM_MAX_PENDING_MINTS_PER_GROUP_U16
                } else {
                    rem
                }
            } else {
                PM_MAX_PENDING_MINTS_PER_GROUP_U16
            };

        let pending_mints_buffer_offset_start = get_nth_pending_mint_offset(
            total_current_pending_mints_groups,
            mint_group_index * PM_MAX_PENDING_MINTS_PER_GROUP_U16,
        );

        let buffer_account = &self.pending_mint_txos.current_pending_mints_tracker.last_finalized_auto_claim_mints_storage_account;
        if buffer_account != mint_buffer_public_key {
            return Err(DogeBridgeError::InvalidAccountKey);
        }
        let can_unlock = self.pending_mint_txos
            .mark_pending_mints_group_claimed(mint_group_index)?;

        for p in 0..mints_count_for_current_group {
            let offset = pending_mints_buffer_offset_start + p as usize * PM_DA_PENDING_MINT_SIZE;
            let pending_mint: &PendingMint = bytemuck::from_bytes(
                &auto_claim_mint_buffer_data_account_memory
                    [offset..(offset + PM_DA_PENDING_MINT_SIZE)],
            );
            minter.mint_to(p as usize, &pending_mint.recipient, pending_mint.amount)?;
        }
        
        

        Ok((can_unlock, mints_count_for_current_group as usize))
    }
    pub fn run_auto_mint_group_old<Minter: MintCPIHelper, UnlockHelper: UnlockAutoClaimMintBufferCPIHelper>(
        &mut self,
        minter: &Minter,
        unlock_helper: &UnlockHelper,
        self_bridge_program_pub_key: &[u8; 32],
        // get auto_claim_mint_buffer_data_account_memory from the self.pending_mint_txos.current_pending_mints_tracker.last_finalized_auto_claim_mints_storage_account
        auto_claim_mint_buffer_data_account_memory: &[u8],
        mint_group_index: u16,
    ) -> QDogeResult<()> {
        if self.pending_mint_txos.is_empty() {
            return Err(DogeBridgeError::NoPendingMintsToAutoProcess);
        }
        let total_current_pending_mints_groups = self
            .pending_mint_txos
            .current_pending_mints_tracker
            .get_current_total_pending_mints_groups();

        if total_current_pending_mints_groups == 0 {
            return Err(DogeBridgeError::NoPendingMintsToAutoProcess);
        }
        if self
            .pending_mint_txos
            .current_pending_mints_tracker
            .is_empty()
        {
            return Err(DogeBridgeError::NoPendingMintsToAutoProcess);
        }
        self.pending_mint_txos
            .current_pending_mints_tracker
            .ensure_can_claim_pending_mints_group(mint_group_index)?;

        let auto_claim_mints_header: &PendingMintsBufferStateHeader = bytemuck::from_bytes(
            &auto_claim_mint_buffer_data_account_memory
                [0..PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE],
        );

        if mint_group_index >= auto_claim_mints_header.pending_mint_groups_count {
            // sanity check
            return Err(DogeBridgeError::PendingMintsGroupIndexOutOfBounds);
        }
        if &auto_claim_mints_header.authorized_locker_public_key != self_bridge_program_pub_key {
            // even though we check this before setting auto_claim_mints_header.authorized_locker_public_key, we do a snaity check
            return Err(DogeBridgeError::InvalidMintBufferLockingPermission);
        }
        let mints_count_for_current_group =
            if mint_group_index == total_current_pending_mints_groups - 1 {
                let rem = self
                    .pending_mint_txos
                    .current_pending_mints_tracker
                    .total_pending_mints as u16
                    % PM_MAX_PENDING_MINTS_PER_GROUP_U16;
                if rem == 0 {
                    PM_MAX_PENDING_MINTS_PER_GROUP_U16
                } else {
                    rem
                }
            } else {
                PM_MAX_PENDING_MINTS_PER_GROUP_U16
            };

        let pending_mints_buffer_offset_start = get_nth_pending_mint_offset(
            total_current_pending_mints_groups,
            mint_group_index * PM_MAX_PENDING_MINTS_PER_GROUP_U16,
        );

        let buffer_account = self.pending_mint_txos.current_pending_mints_tracker.last_finalized_auto_claim_mints_storage_account;
        let can_unlock = self.pending_mint_txos
            .mark_pending_mints_group_claimed(mint_group_index)?;

        for p in 0..mints_count_for_current_group {
            let offset = pending_mints_buffer_offset_start + p as usize * PM_DA_PENDING_MINT_SIZE;
            let pending_mint: &PendingMint = bytemuck::from_bytes(
                &auto_claim_mint_buffer_data_account_memory
                    [offset..(offset + PM_DA_PENDING_MINT_SIZE)],
            );
            minter.mint_to(p as usize, &pending_mint.recipient, pending_mint.amount)?;
        }
        
        if can_unlock {
            unlock_helper.unlock_buffer(&buffer_account)?;
        }

        Ok(())
    }
    pub fn run_setup_next_pending_buffer(
        &mut self,
        self_bridge_program_pub_key: &[u8; 32],
        mint_buffer_public_key: [u8; 32],

        auto_claim_txo_buffer_data_account_memory: &[u8],
        auto_claim_mint_buffer_data_account_memory: &[u8],
    ) -> QDogeResult<()> {
        if self.is_ready_for_new_block_update() || self.pending_mint_txos.is_empty() {
            return Err(DogeBridgeError::NoPendingMintsToAutoProcess);
        }
        if !self
            .pending_mint_txos
            .current_pending_mints_tracker
            .is_empty()
        {
            return Err(DogeBridgeError::RemainingPendingMintsInPreviousState);
        }
        self.pending_mint_txos
            .ensure_ready_to_start_next_pending_finalized_info()?;
        self.pending_mint_txos.fast_forward_empty();
        if self.is_ready_for_new_block_update() || self.pending_mint_txos.is_empty() {
            return Err(DogeBridgeError::NoPendingMintsToAutoProcess);
        }
        if !self
            .pending_mint_txos
            .current_pending_mints_tracker
            .is_empty()
        {
            return Err(DogeBridgeError::RemainingPendingMintsInPreviousState);
        }
        self.pending_mint_txos
            .ensure_ready_to_start_next_pending_finalized_info()?;

        // advance to the next pending mint buffer
        // ensuring that the hashes of the txo buffer and mint buffer in our program's state match the ones passed in

        let pmfh = self.pending_mint_txos.pending_finalized_info
            [self.pending_mint_txos.pending_finalized_info_current_index as usize]
            .pending_mints_finalized_hash;

        let (pending_mints_buffer_hash, pending_mints_count) = self
            .ensure_pending_mints_ready_for_transition_inner(
                self_bridge_program_pub_key,
                &pmfh,
                auto_claim_mint_buffer_data_account_memory,
            )?;

        let pm_txo_data_account_size = pm_txo_data_account_min_size(pending_mints_count);
        if auto_claim_txo_buffer_data_account_memory.len() < pm_txo_data_account_size {
            return Err(DogeBridgeError::InvalidAutoClaimTxoBufferDataAccountSize);
        }
        let txo_header = bytemuck::from_bytes::<PendingMintsTxoBufferHeader>(
            &auto_claim_txo_buffer_data_account_memory[0..PM_TXO_BUFFER_HEADER_SIZE],
        );
        // make sure the txo buffer is tagged for the correct block height
        if txo_header.doge_block_height != (self.pending_mint_txos.start_block_height + self.pending_mint_txos.pending_finalized_info_current_index as u32) {
            return Err(DogeBridgeError::InvalidBlockHeightForReorgTransition);
        }

        // hash of all the combined txo indicies
        let txo_hash = hash_impl_sha256_bytes(
            &auto_claim_txo_buffer_data_account_memory
                [PM_TXO_BUFFER_HEADER_SIZE..pm_txo_data_account_size],
        );
        let new_pfi = &self.pending_mint_txos.pending_finalized_info
            [self.pending_mint_txos.pending_finalized_info_current_index as usize];

        if txo_hash != new_pfi.txo_output_list_finalized_hash {
            return Err(DogeBridgeError::InvalidAutoClaimTxoBufferHash);
        }
        if new_pfi.pending_mints_finalized_hash != pending_mints_buffer_hash {
            return Err(DogeBridgeError::InvalidPendingMintsBufferHash);
        }

        let (mint_groups, _) = compute_mint_group_info(pending_mints_count);

        // all checks passed, we can now update our state to reflect the new pending mint buffer
        self.pending_mint_txos.current_pending_mints_tracker = PendingMintsTracker {
            last_finalized_auto_claim_mints_storage_account: mint_buffer_public_key,
            pending_mint_groups_claimed: [0u8; 32],
            total_pending_mints: pending_mints_count as u32,
            pending_mints_groups_remaining: mint_groups as u32,
        };

        Ok(())
    }
}
