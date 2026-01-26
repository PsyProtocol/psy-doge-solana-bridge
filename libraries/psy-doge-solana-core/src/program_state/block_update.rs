use psy_bridge_core::{
    common_types::QHash256,
    crypto::{
        hash::sha256_impl::hash_impl_sha256_bytes,
        zk::{
            CompactZKProofVerifier,
            COMPACT_BRIDGE_ZK_PROOF_SIZE, COMPACT_BRIDGE_ZK_VERIFIER_KEY_SIZE,
        },
    },
    error::{DogeBridgeError, QDogeResult},
    header::PsyBridgeHeader,
};

use crate::{
    data_accounts::pending_mint::{
        PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH, PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE, PM_MAX_PENDING_MINTS_PER_GROUP_U16, PM_TXO_BUFFER_HEADER_SIZE, PendingMintsBufferStateHeader, PendingMintsTxoBufferHeader, pm_calculate_data_account_min_size, pm_txo_data_account_min_size
    },
    program_state::{FinalizedBlockMintTxoInfo, PsyBridgeProgramState},
    public_inputs::{get_block_transition_public_inputs, get_reorg_block_transition_public_inputs},
};
// returns (total groups, size of last group)
pub fn compute_mint_group_info(total_mints: u16) -> (u16, u16) {
    if total_mints == 0 {
        return (0, 0);
    }
    let rem = total_mints % PM_MAX_PENDING_MINTS_PER_GROUP_U16;
    let last_group_size = if rem == 0 {
        PM_MAX_PENDING_MINTS_PER_GROUP_U16
    } else {
        rem
    };
    if rem == 0 {
        (
            total_mints / PM_MAX_PENDING_MINTS_PER_GROUP_U16,
            last_group_size,
        )
    } else {
        (
            total_mints / PM_MAX_PENDING_MINTS_PER_GROUP_U16 + 1,
            last_group_size,
        )
    }
}

fn get_backlog_contains_pending_mints(
    pending_backlog: &[&FinalizedBlockMintTxoInfo],
) -> Option<usize> {
    for (i, item) in pending_backlog.iter().enumerate() {
        if !item.is_empty() {
            return Some(i);
        }
    }
    None
}
impl PsyBridgeProgramState {
    pub fn is_ready_for_new_block_update(&self) -> bool {
        self.pending_mint_txos.is_empty()
    }
    pub fn ensure_pending_mints_ready_for_transition_inner(
        &mut self,
        self_bridge_program_pub_key: &[u8; 32],
        expected_pending_mints_buffer_hash: &QHash256,
        auto_claim_mint_buffer_data_account_memory: &[u8],
    ) -> QDogeResult<(QHash256, u16)> {
        if auto_claim_mint_buffer_data_account_memory.len()
            < PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE
        {
            return Err(DogeBridgeError::InvalidAutoClaimMintBufferDataAccountSize);
        }
        let mint_buffer_header = bytemuck::from_bytes::<PendingMintsBufferStateHeader>(
            &auto_claim_mint_buffer_data_account_memory
                [0..PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE],
        );

        if &mint_buffer_header.authorized_locker_public_key != self_bridge_program_pub_key {
            return Err(DogeBridgeError::InvalidMintBufferLockingPermission);
        }
        let pending_mints_count = mint_buffer_header.pending_mints_count;

        let (mint_groups, _) = compute_mint_group_info(pending_mints_count);

        if mint_buffer_header.pending_mint_groups_count != mint_groups {
            return Err(DogeBridgeError::InvalidMintBufferPendingMintGroupsCount);
        }
        if pending_mints_count >= u16::MAX
            || mint_buffer_header.pending_mints_count != pending_mints_count
        {
            return Err(DogeBridgeError::InvalidMintBufferPendingMintsCount);
        }

        let expected_mint_buffer_min_data_account_size =
            pm_calculate_data_account_min_size(pending_mints_count);
        if auto_claim_mint_buffer_data_account_memory.len()
            < expected_mint_buffer_min_data_account_size
        {
            return Err(DogeBridgeError::InvalidAutoClaimMintBufferDataAccountSize);
        }

        // we also hash the total mints u16 at the end of the header
        let hash_preimage_slice =
            &auto_claim_mint_buffer_data_account_memory[70..(mint_groups as usize * 32 + 72)];

        let pending_mints_buffer_hash = hash_impl_sha256_bytes(&hash_preimage_slice);
        if expected_pending_mints_buffer_hash != &pending_mints_buffer_hash {
            return Err(DogeBridgeError::InvalidPendingMintsBufferHash);
        }
        Ok((pending_mints_buffer_hash, pending_mints_count))
    }
    pub fn ensure_pending_mints_ready_for_transition(
        &mut self,
        pending_mints_count: u16,
        self_bridge_program_pub_key: &[u8; 32],
        expected_pending_mints_buffer_hash: &QHash256,
        auto_claim_mint_buffer_data_account_memory: &[u8],
    ) -> QDogeResult<(QHash256, bool)> {
        if pending_mints_count == 0 {
            return Ok((PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH, false));
        }
        let (pending_mints_buffer_hash, actual_pending_mints_count) = self
            .ensure_pending_mints_ready_for_transition_inner(
                self_bridge_program_pub_key,
                expected_pending_mints_buffer_hash,
                auto_claim_mint_buffer_data_account_memory,
            )?;
        if actual_pending_mints_count != pending_mints_count {
            return Err(DogeBridgeError::InvalidPendingMintsCountForTransition);
        }
        Ok((pending_mints_buffer_hash, pending_mints_count > 0))
    }
    pub fn run_standard_single_block_transition<
        ZKVerifier: CompactZKProofVerifier,
    >(
        &mut self,
        proof: &[u8],
        vk: &[u8],
        new_header: &PsyBridgeHeader,
        self_bridge_program_pub_key: &[u8; 32],
        mint_buffer_storage_account_pub_key: [u8; 32],
        auto_claim_txo_buffer_data_account_memory: &[u8],
        auto_claim_mint_buffer_data_account_memory: &[u8],
    ) -> QDogeResult<()> {
        if !self.is_ready_for_new_block_update() {
            return Err(DogeBridgeError::ProgramStateNotReadyForBlockUpdate);
        }
        if proof.len() != COMPACT_BRIDGE_ZK_PROOF_SIZE {
            return Err(DogeBridgeError::InvalidZKProofSize);
        }
        if vk.len() != COMPACT_BRIDGE_ZK_VERIFIER_KEY_SIZE {
            return Err(DogeBridgeError::InvalidZKVerifierKeySize);
        }
        if new_header.finalized_state.block_height
            != self.bridge_header.finalized_state.block_height + 1
        {
            return Err(DogeBridgeError::InvalidBlockHeightForSingleBlockTransition);
        }
        if new_header.tip_state.block_height <= self.bridge_header.tip_state.block_height {
            return Err(DogeBridgeError::InvalidBlockHeightForSingleBlockTransition);
        }

        if new_header.finalized_state.auto_claimed_deposits_next_index
            < self
                .bridge_header
                .finalized_state
                .auto_claimed_deposits_next_index
        {
            return Err(DogeBridgeError::InvalidAutoClaimedDepositsNextIndex);
        }
        let total_new_auto_claims = new_header.finalized_state.auto_claimed_deposits_next_index
            - self
                .bridge_header
                .finalized_state
                .auto_claimed_deposits_next_index;

        if total_new_auto_claims > u16::MAX as u32 {
            return Err(DogeBridgeError::TooManyNewAutoClaimedDeposits);
        }
        let total_new_auto_claims = total_new_auto_claims as u16;

        // check zkp

        let previous_header_hash = self.bridge_header.get_hash_canonical();
        let new_header_hash = new_header.get_hash_canonical();
        let expected_zkp_public_inputs = get_block_transition_public_inputs(
            &previous_header_hash,
            &new_header_hash,
            &self.config_params.get_hash(),
            &self.custodian_wallet_config_hash,
        );

        let is_zkp_valid =
            ZKVerifier::verify_compact_zkp_slice(proof, vk, &expected_zkp_public_inputs);
        if !is_zkp_valid {
            return Err(DogeBridgeError::InvalidBridgeInputZKP);
        }

        let (pending_mints_buffer_hash, has_new_auto_mints) = self
            .ensure_pending_mints_ready_for_transition(
                total_new_auto_claims,
                self_bridge_program_pub_key,
                &new_header.finalized_state.pending_mints_finalized_hash,
                auto_claim_mint_buffer_data_account_memory,
            )?;
        if pending_mints_buffer_hash != new_header.finalized_state.pending_mints_finalized_hash {
            return Err(DogeBridgeError::InvalidPendingMintsBufferHash);
        }
        if has_new_auto_mints {
            let pm_txo_data_account_size = pm_txo_data_account_min_size(total_new_auto_claims);
            if auto_claim_txo_buffer_data_account_memory.len() < pm_txo_data_account_size {
                return Err(DogeBridgeError::InvalidAutoClaimTxoBufferDataAccountSize);
            }
            let txo_header = bytemuck::from_bytes::<PendingMintsTxoBufferHeader>(
                &auto_claim_txo_buffer_data_account_memory[0..PM_TXO_BUFFER_HEADER_SIZE],
            );
            if txo_header.doge_block_height != new_header.finalized_state.block_height || txo_header.data_size == 0 {
                // make sure it is tagged with the correct block height if there are any new mints
                return Err(DogeBridgeError::InvalidAutoClaimTxoBufferPendingMintsCount);
            }

            // hash of all the combined txo indicies
            let txo_hash = hash_impl_sha256_bytes(
                &auto_claim_txo_buffer_data_account_memory
                    [PM_TXO_BUFFER_HEADER_SIZE..pm_txo_data_account_size],
            );

            if txo_hash != new_header.finalized_state.txo_output_list_finalized_hash {
                return Err(DogeBridgeError::InvalidAutoClaimTxoBufferHash);
            }

            // check mint bridge locker
            let (mint_groups, _) = compute_mint_group_info(total_new_auto_claims);

            self.pending_mint_txos.standard_append_block(
                new_header.finalized_state.block_height,
                &[&FinalizedBlockMintTxoInfo {
                    pending_mints_finalized_hash: pending_mints_buffer_hash,
                    txo_output_list_finalized_hash: txo_hash,
                }],
                mint_buffer_storage_account_pub_key,
                total_new_auto_claims as u32,
                mint_groups as u32,
            )?;
        }
        self.bridge_header = new_header.clone();
        self.recent_finalized_blocks[self.next_recent_finalized_block_index as usize] =
            new_header.finalized_state;
        self.next_recent_finalized_block_index = (self.next_recent_finalized_block_index + 1) % 8;
        Ok(())
    }

    pub fn run_block_transition_reorg<
        ZKVerifier: CompactZKProofVerifier,
    >(
        &mut self,
        proof: &[u8],
        vk: &[u8],
        new_header: &PsyBridgeHeader,
        extra_finalized_blocks: &[&FinalizedBlockMintTxoInfo],
        self_bridge_program_pub_key: &[u8; 32],
        mint_buffer_locker_account_pubkey: [u8; 32],
        auto_claim_txo_buffer_data_account_memory: &[u8],
        auto_claim_mint_buffer_data_account_memory: &[u8],
    ) -> QDogeResult<()> {
        if !self.is_ready_for_new_block_update() {
            return Err(DogeBridgeError::ProgramStateNotReadyForBlockUpdate);
        }
        if proof.len() != COMPACT_BRIDGE_ZK_PROOF_SIZE {
            return Err(DogeBridgeError::InvalidZKProofSize);
        }
        if vk.len() != COMPACT_BRIDGE_ZK_VERIFIER_KEY_SIZE {
            return Err(DogeBridgeError::InvalidZKVerifierKeySize);
        }
        // we do not allow reorgs to previous blocks or reorgs which have not exceeded the current finalized block height
        if new_header.finalized_state.block_height
            <= self.bridge_header.finalized_state.block_height
        {
            return Err(DogeBridgeError::InvalidBlockHeightForSingleBlockTransition);
        }
        if new_header.tip_state.block_height <= self.bridge_header.tip_state.block_height {
            return Err(DogeBridgeError::InvalidBlockHeightForSingleBlockTransition);
        }

        let fast_forward_size = new_header.finalized_state.block_height
            - self.bridge_header.finalized_state.block_height;
        if extra_finalized_blocks.len() != (fast_forward_size - 1) as usize {
            return Err(DogeBridgeError::InvalidExtraFinalizedBlocksLengthForReorg);
        }

        let extra_fast_forwarded = fast_forward_size - 1;
        if extra_fast_forwarded != (extra_finalized_blocks.len() as u32) {
            return Err(DogeBridgeError::InvalidExtraFinalizedBlocksLengthForReorg);
        }

        if new_header.finalized_state.auto_claimed_deposits_next_index
            < (self
                .bridge_header
                .finalized_state
                .auto_claimed_deposits_next_index)
        {
            return Err(DogeBridgeError::InvalidAutoClaimedDepositsNextIndex);
        }
        // check zkp

        let previous_header_hash = self.bridge_header.get_hash_canonical();
        let new_header_hash = new_header.get_hash_canonical();
        let expected_zkp_public_inputs = get_reorg_block_transition_public_inputs(
            &previous_header_hash,
            &new_header_hash,
            extra_finalized_blocks,
            &self.config_params.get_hash(),
            &self.custodian_wallet_config_hash,
        );
        let is_zkp_valid =
            ZKVerifier::verify_compact_zkp_slice(proof, vk, &expected_zkp_public_inputs);
        if !is_zkp_valid {
            return Err(DogeBridgeError::InvalidBridgeInputZKP);
        }

        let last_block_info = FinalizedBlockMintTxoInfo {
            pending_mints_finalized_hash: new_header.finalized_state.pending_mints_finalized_hash,
            txo_output_list_finalized_hash: new_header
                .finalized_state
                .txo_output_list_finalized_hash,
        };
        let mut new_items = Vec::with_capacity(extra_finalized_blocks.len() + 1);
        new_items.extend_from_slice(extra_finalized_blocks);
        new_items.push(&last_block_info);

        let first_non_empty_in_backlog = get_backlog_contains_pending_mints(&new_items);
        if first_non_empty_in_backlog.is_none() {
            // no new pending mints to process
            self.bridge_header = new_header.clone();
            return Ok(());
        }

        let first_non_empty_in_backlog_index = first_non_empty_in_backlog.unwrap();

        let (pending_mints_buffer_hash, pending_mints_count) = self
            .ensure_pending_mints_ready_for_transition_inner(
                self_bridge_program_pub_key,
                &new_items[first_non_empty_in_backlog_index].pending_mints_finalized_hash,
                auto_claim_mint_buffer_data_account_memory,
            )?;

        if pending_mints_buffer_hash
            != new_items[first_non_empty_in_backlog_index].pending_mints_finalized_hash
        {
            return Err(DogeBridgeError::InvalidPendingMintsBufferHash);
        }
        let pm_txo_data_account_size = pm_txo_data_account_min_size(pending_mints_count);
        if auto_claim_txo_buffer_data_account_memory.len() < pm_txo_data_account_size {
            return Err(DogeBridgeError::InvalidAutoClaimTxoBufferDataAccountSize);
        }
        let txo_header = bytemuck::from_bytes::<PendingMintsTxoBufferHeader>(
            &auto_claim_txo_buffer_data_account_memory[0..PM_TXO_BUFFER_HEADER_SIZE],
        );
        let first_block_height_in_backlog = new_header.finalized_state.block_height
            - (new_items.len() as u32 - first_non_empty_in_backlog_index as u32 - 1);
        if txo_header.doge_block_height != first_block_height_in_backlog {
            // make sure it is tagged with the correct block height
            return Err(DogeBridgeError::InvalidAutoClaimTxoBufferPendingMintsCount);
        }

        // hash of all the combined txo indicies
        let txo_hash = hash_impl_sha256_bytes(
            &auto_claim_txo_buffer_data_account_memory
                [PM_TXO_BUFFER_HEADER_SIZE..pm_txo_data_account_size],
        );

        if txo_hash != new_items[first_non_empty_in_backlog_index].txo_output_list_finalized_hash {
            return Err(DogeBridgeError::InvalidAutoClaimTxoBufferHash);
        }

        // check mint bridge locker
        let (mint_groups, _) = compute_mint_group_info(pending_mints_count);

        self.pending_mint_txos.standard_append_block(
            first_block_height_in_backlog, // FIX: Pass the height of the first block being processed
            &new_items[first_non_empty_in_backlog_index..],
            mint_buffer_locker_account_pubkey,
            pending_mints_count as u32,
            mint_groups as u32,
        )?;

        self.bridge_header = new_header.clone();
        Ok(())
    }
}