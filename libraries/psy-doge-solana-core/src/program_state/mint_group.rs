use psy_bridge_core::{common_types::QHash256, error::{DogeBridgeError, QDogeResult}};

use crate::data_accounts::pending_mint::{PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH, PM_MAX_PENDING_MINTS_PER_GROUP_U16, PM_TXO_DEFAULT_BUFFER_HASH};


#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct PendingMintsTracker {
    pub last_finalized_auto_claim_mints_storage_account: [u8; 32],
    pub pending_mint_groups_claimed: [u8; 32], // bitmap of pending mint groups that are not yet finalized
    pub total_pending_mints: u32,
    pub pending_mints_groups_remaining: u32,

}
impl PendingMintsTracker {
    pub fn is_empty(&self) -> bool {
        self.pending_mints_groups_remaining == 0
    }
    pub fn is_pending_mints_group_already_claimed(&self, group_index: u16) -> bool {
        let byte_index = group_index >> 3;
        let bit = (self.pending_mint_groups_claimed[byte_index as usize] >> (group_index&7))&1;
        bit == 1
    }
    pub fn get_current_total_pending_mints_groups(&self) -> u16 {
        if self.total_pending_mints as u16 %  PM_MAX_PENDING_MINTS_PER_GROUP_U16 == 0 {
            self.total_pending_mints as u16 / PM_MAX_PENDING_MINTS_PER_GROUP_U16
        }else{
            self.total_pending_mints as u16 / PM_MAX_PENDING_MINTS_PER_GROUP_U16 + 1
        }
    }
    pub fn ensure_can_claim_pending_mints_group(&self, group_index: u16) -> QDogeResult<()> {
        if self.total_pending_mints == 0 || self.pending_mints_groups_remaining == 0{
            Err(DogeBridgeError::NoPendingMintsToProcess)
        }else if group_index > 256 {
            Err(DogeBridgeError::PendingMintsGroupIndexOutOfBounds)
        }else if group_index >= self.get_current_total_pending_mints_groups() {
            Err(DogeBridgeError::PendingMintsGroupIndexOutOfBounds)
        }else if self.is_pending_mints_group_already_claimed(group_index){
            Err(DogeBridgeError::PendingMintsGroupAlreadyProcessed)
        }else{
            Ok(())
        }
    }
    pub fn mark_pending_mints_group_claimed(&mut self, group_index: u16) -> QDogeResult<bool> {
        if self.total_pending_mints <= 0 || self.pending_mints_groups_remaining <= 0{
            Err(DogeBridgeError::NoPendingMintsToProcess)
        }else if group_index > 256 {
            Err(DogeBridgeError::PendingMintsGroupIndexOutOfBounds)
        }else if self.is_pending_mints_group_already_claimed(group_index){
            Err(DogeBridgeError::PendingMintsGroupAlreadyProcessed)
        }else{
            let byte_index = group_index >> 3;
            self.pending_mint_groups_claimed[byte_index as usize] |= 1 << (group_index&7);
            self.pending_mints_groups_remaining -= 1;
            Ok(self.pending_mints_groups_remaining == 0)
        }
    }

}

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct FinalizedBlockMintTxoInfo {
    pub pending_mints_finalized_hash: QHash256,
    pub txo_output_list_finalized_hash: QHash256,
}



impl FinalizedBlockMintTxoInfo {
    pub fn is_empty(&self) -> bool {
        self.pending_mints_finalized_hash == PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH && self.txo_output_list_finalized_hash == PM_TXO_DEFAULT_BUFFER_HASH
    }
}

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct FinalizedBlockMintTxoManager {
    pub pending_finalized_info: [FinalizedBlockMintTxoInfo; 8], // you can only go 8 blocks ahead of finalized at any given time
    pub pending_finalized_info_current_index: u16,
    pub pending_finalized_info_total_count: u16,
    pub start_block_height: u32,
    pub current_pending_mints_tracker: PendingMintsTracker,
}

impl FinalizedBlockMintTxoManager {
    pub fn reset(&mut self) {
        self.pending_finalized_info = [FinalizedBlockMintTxoInfo {
            pending_mints_finalized_hash: QHash256::default(),
            txo_output_list_finalized_hash: QHash256::default(),
        }; 8];
        self.pending_finalized_info_current_index = 0;
        self.pending_finalized_info_total_count = 0;
        self.start_block_height = 0;
        self.current_pending_mints_tracker = PendingMintsTracker {
            last_finalized_auto_claim_mints_storage_account: [0u8; 32],
            pending_mint_groups_claimed: [0u8; 32],
            total_pending_mints: 0,
            pending_mints_groups_remaining: 0,
        };
    }
    pub fn new_empty() -> Self {
        Self::default()
    }
    pub fn is_empty(&self) -> bool {
        self.pending_finalized_info_total_count == 0 || (self.pending_finalized_info_total_count == self.pending_finalized_info_current_index && self.current_pending_mints_tracker.is_empty())
    }
    pub fn fast_forward_empty(&mut self) {
        if self.pending_finalized_info_current_index >= self.pending_finalized_info_total_count || !self.current_pending_mints_tracker.is_empty() || self.is_empty(){
            return;
        }
        let mut modified = false;
        for i in self.pending_finalized_info_current_index..self.pending_finalized_info_total_count {
            if !self.pending_finalized_info[i as usize].is_empty() {
                break;
            }
            self.pending_finalized_info_current_index += 1;
            modified = true;
        }
        if modified {
            self.current_pending_mints_tracker = PendingMintsTracker {
                last_finalized_auto_claim_mints_storage_account: [0u8; 32],
                pending_mint_groups_claimed: [0u8; 32],
                total_pending_mints: 0,
                pending_mints_groups_remaining: 0,
            };
        }
    }
    

    pub fn mark_pending_mints_group_claimed(&mut self, group_index: u16) -> QDogeResult<bool> {
        let result = self.current_pending_mints_tracker.mark_pending_mints_group_claimed(group_index)?;
        if result {
            // Increment the index when current block is fully claimed, regardless of whether it's the last block in the backlog.
            if self.pending_finalized_info_current_index < self.pending_finalized_info_total_count {
                self.pending_finalized_info_current_index += 1;
            }
        }
        Ok(result)
    }
    pub fn ensure_ready_to_start_next_pending_finalized_info(&self) -> QDogeResult<()> {
        if !self.current_pending_mints_tracker.is_empty() {
            return Err(DogeBridgeError::RemainingPendingMintsInPreviousState);
        }
        if self.pending_finalized_info_current_index >= self.pending_finalized_info_total_count {
            return Err(DogeBridgeError::NoPendingMintsToAutoProcess);
        }
        Ok(())
    }
    pub fn standard_append_block(
        &mut self,
        block_height: u32,
        block_groups: &[&FinalizedBlockMintTxoInfo],
        auto_claim_mints_storage_account: [u8; 32],
        total_pending_mints_for_first_block: u32,
        total_pending_mints_groups_for_first_block: u32,
    ) -> QDogeResult<()> {
        
        if !self.is_empty() {
            return Err(DogeBridgeError::PendingFinalizedBlockMintsNotEmpty.into());
        }
        
        if block_groups.len() > 8 {
            return Err(DogeBridgeError::PendingFinalizedBlockMintsInvalidGroupCount.into());
        }else if block_groups.len() == 0 {
            return Ok(());
        }
        for i in 0..block_groups.len() {
            self.pending_finalized_info[i] = *block_groups[i];
        }
        self.pending_finalized_info_current_index = 0;
        self.pending_finalized_info_total_count = block_groups.len() as u16;
        self.start_block_height = block_height;
        self.current_pending_mints_tracker = PendingMintsTracker {
            last_finalized_auto_claim_mints_storage_account: auto_claim_mints_storage_account,
            pending_mint_groups_claimed: [0u8; 32],
            total_pending_mints: 0,
            pending_mints_groups_remaining: 0,
        };
        self.fast_forward_empty();
        if self.is_empty() {
            // sanity check, we should have been passed zero data if there are no pending mints
            if total_pending_mints_for_first_block != 0 || total_pending_mints_groups_for_first_block != 0 {
                return Err(DogeBridgeError::PendingFinalizedBlockMintsInvalidAutoClaimMintsData.into());
            }
            return Ok(());
        }
        self.current_pending_mints_tracker.total_pending_mints = total_pending_mints_for_first_block;
        self.current_pending_mints_tracker.pending_mints_groups_remaining = total_pending_mints_groups_for_first_block;


        Ok(())
    }

}