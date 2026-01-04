use psy_bridge_core::common_types::QHash256;
use psy_bridge_core::crypto::zk::CompactBridgeZKProof;

#[derive(bytemuck::Pod, bytemuck::Zeroable, Clone, Copy, Debug)]
#[repr(C)]
pub struct ManualClaimInstruction {
    pub proof: CompactBridgeZKProof,
    pub recent_block_merkle_tree_root: QHash256,
    pub recent_auto_claim_txo_root: QHash256,
    pub new_manual_claim_txo_root: QHash256,
    pub tx_hash: QHash256,
    pub combined_txo_index: u64,
    pub deposit_amount_sats: u64,
}
