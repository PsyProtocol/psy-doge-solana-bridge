use psy_bridge_core::common_types::QHash256;
use psy_bridge_core::crypto::zk::CompactBridgeZKProof;

pub const MC_MANUAL_CLAIM_TRANSACTION_DESCRIMINATOR: u8 = 0;

#[macro_rules_attribute::apply(crate::DeriveCopySerializeReprC)]
pub struct ManualClaimInstruction {
    #[cfg_attr(feature = "serialize_serde", serde(with = "psy_bridge_core::serde_arrays::serde_arrays"))]
    pub proof: CompactBridgeZKProof,
    pub recent_block_merkle_tree_root: QHash256,
    pub recent_auto_claim_txo_root: QHash256,
    pub new_manual_claim_txo_root: QHash256,
    pub tx_hash: QHash256,
    pub combined_txo_index: u64,
    pub deposit_amount_sats: u64,
}

impl Default for ManualClaimInstruction {
    fn default() -> Self {
        Self {
            proof: [0u8; 256],
            recent_block_merkle_tree_root: [0u8; 32],
            recent_auto_claim_txo_root: [0u8; 32],
            new_manual_claim_txo_root: [0u8; 32],
            tx_hash: [0u8; 32],
            combined_txo_index: 0,
            deposit_amount_sats: 0,
        }
    }
}