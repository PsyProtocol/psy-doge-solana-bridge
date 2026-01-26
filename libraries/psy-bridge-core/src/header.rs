use crate::{common_types::QHash256, crypto::hash::sha256_impl::hash_impl_sha256_bytes};


#[cfg_attr(feature = "serialize_serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize_borsh", derive(borsh::BorshSerialize, borsh::BorshDeserialize))]
#[cfg_attr(feature = "serialize_speedy", derive(speedy::Readable, speedy::Writable))]
#[cfg_attr(feature = "serialize_bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
#[derive(PartialEq, Clone, Debug, Eq, Ord, PartialOrd, Copy, Hash, Default)]
#[repr(C)]
pub struct PsyBridgeStateCommitment {
    pub block_hash: QHash256,
    pub block_merkle_tree_root: QHash256,
    pub pending_mints_finalized_hash: QHash256,
    pub txo_output_list_finalized_hash: QHash256,
    pub auto_claimed_txo_tree_root: QHash256,
    pub auto_claimed_deposits_tree_root: QHash256,
    pub auto_claimed_deposits_next_index: u32,
    pub block_height: u32,
}


#[cfg_attr(feature = "serialize_serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize_borsh", derive(borsh::BorshSerialize, borsh::BorshDeserialize))]
#[cfg_attr(feature = "serialize_speedy", derive(speedy::Readable, speedy::Writable))]
#[cfg_attr(feature = "serialize_bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
#[derive(PartialEq, Clone, Debug, Eq, Ord, PartialOrd, Copy, Hash, Default)]
#[repr(C)]
pub struct PsyBridgeTipStateCommitment {
    pub block_hash: QHash256,
    pub block_merkle_tree_root: QHash256,
    pub block_time: u32,
    pub block_height: u32,
}
#[cfg_attr(feature = "serialize_serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize_borsh", derive(borsh::BorshSerialize, borsh::BorshDeserialize))]
#[cfg_attr(feature = "serialize_speedy", derive(speedy::Readable, speedy::Writable))]
#[cfg_attr(feature = "serialize_bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
#[derive(PartialEq, Clone, Debug, Eq, Ord, PartialOrd, Copy, Hash, Default)]
#[repr(C)]
pub struct PsyBridgeHeaderUpdate {
    pub tip_state: PsyBridgeTipStateCommitment,
    pub finalized_block_hash: QHash256,
    pub finalized_block_merkle_tree_root: QHash256,
    pub finalized_auto_claimed_txo_tree_root: QHash256,
    pub finalized_auto_claimed_deposits_tree_root: QHash256,
    pub bridge_state_hash: QHash256, // hash of QEDDogeChainStateCore, useful for other contracts that want to introspect dogecoin's blocks
    pub last_rollback_at_secs: u32,
    pub paused_until_secs: u32,
    pub total_finalized_fees_collected_chain_history: u64,
}
impl PsyBridgeHeaderUpdate {
    pub fn to_header(&self, 
        required_confirmations: u32, 
        pending_mints_finalized_hash: QHash256,
        txo_output_list_finalized_hash: QHash256,
        auto_claimed_deposits_next_index: u32,
) -> PsyBridgeHeader {
        PsyBridgeHeader {
            tip_state: self.tip_state,
            finalized_state: PsyBridgeStateCommitment {
                block_hash: self.finalized_block_hash,
                block_merkle_tree_root: self.finalized_block_merkle_tree_root,
                block_height: self.tip_state.block_height.checked_sub(required_confirmations).unwrap_or(0),
                pending_mints_finalized_hash,
                txo_output_list_finalized_hash,
                auto_claimed_txo_tree_root: self.finalized_auto_claimed_txo_tree_root,
                auto_claimed_deposits_tree_root: self.finalized_auto_claimed_deposits_tree_root,
                auto_claimed_deposits_next_index,
            },
            bridge_state_hash: self.bridge_state_hash,
            last_rollback_at_secs: self.last_rollback_at_secs,
            paused_until_secs: self.paused_until_secs,
            total_finalized_fees_collected_chain_history: self.total_finalized_fees_collected_chain_history,
        }
    }
}
#[cfg_attr(feature = "serialize_serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize_borsh", derive(borsh::BorshSerialize, borsh::BorshDeserialize))]
#[cfg_attr(feature = "serialize_speedy", derive(speedy::Readable, speedy::Writable))]
#[cfg_attr(feature = "serialize_bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
#[derive(PartialEq, Clone, Debug, Eq, Ord, PartialOrd, Copy, Hash, Default)]
#[repr(C)]
pub struct PsyBridgeHeader {
    pub tip_state: PsyBridgeTipStateCommitment,
    pub finalized_state: PsyBridgeStateCommitment,
    pub bridge_state_hash: QHash256, // hash of QEDDogeChainStateCore, useful for other contracts that want to introspect dogecoin's blocks
    pub last_rollback_at_secs: u32,
    pub paused_until_secs: u32,
    pub total_finalized_fees_collected_chain_history: u64,
}

impl PsyBridgeHeader {
    pub fn copy_from(&mut self, other: &PsyBridgeHeader) {
        self.tip_state = other.tip_state;
        self.finalized_state = other.finalized_state;
        self.bridge_state_hash = other.bridge_state_hash;
        self.last_rollback_at_secs = other.last_rollback_at_secs;
        self.paused_until_secs = other.paused_until_secs;
        self.total_finalized_fees_collected_chain_history =
            other.total_finalized_fees_collected_chain_history;
    }
    pub fn is_paused(&self, current_unix_timestamp_secs: u32) -> bool {
        self.paused_until_secs > current_unix_timestamp_secs
    }

    #[cfg(feature = "serialize_bytemuck")]
    pub fn get_hash_bm(&self) -> QHash256 {
        hash_impl_sha256_bytes(bytemuck::bytes_of(self))
    }

    #[cfg(feature = "serialize_bytemuck")]
    pub fn from_bytes_ref_or_panic_bm(bytes: &[u8]) -> &Self {
        bytemuck::from_bytes::<Self>(bytes)
    }

    #[cfg(feature = "serialize_bytemuck")]
    pub fn to_bytes_ref_bm(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }

    pub fn to_canonical_bytes_ref(&self) -> &[u8] {
        #[cfg(feature = "serialize_bytemuck")]
        {
            self.to_bytes_ref_bm()
        }
        #[cfg(not(feature = "serialize_bytemuck"))]
        {
            panic!("to_canonical_bytes_ref requires 'serialize_bytemuck' feature");
        }
    }
    pub fn get_hash_canonical(&self) -> QHash256 {
        #[cfg(feature = "serialize_bytemuck")]
        {
            self.get_hash_bm()
        }
        #[cfg(not(feature = "serialize_bytemuck"))]
        {
            panic!("get_hash_canonical requires 'serialize_bytemuck' feature");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_bridge_header_hash() {
        let header = PsyBridgeHeader {
            tip_state: PsyBridgeTipStateCommitment::default(),
            finalized_state: PsyBridgeStateCommitment::default(),
            bridge_state_hash: QHash256::default(),
            last_rollback_at_secs: 0,
            paused_until_secs: 0,
            total_finalized_fees_collected_chain_history: 5000,
            //transition_auto_process_mint_hash_stack: QHash256::default(),
        };
        let hash = header.get_hash_bm();

        println!("Bridge Header Hash: {}", hex::encode(hash));
    }
}
