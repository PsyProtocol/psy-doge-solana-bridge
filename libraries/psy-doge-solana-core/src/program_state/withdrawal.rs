use psy_bridge_core::{common_types::QHash256, crypto::hash::sha256_impl::hash_impl_sha256_bytes};


#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct PsyWithdrawalRequest {
    pub recipient_address: [u8; 20],
    pub address_type: u32, // 0 = P2PKH, 1 = P2SH
    pub amount_sats: u64,
}
impl PsyWithdrawalRequest {
    pub fn new(recipient_address: [u8; 20], amount_sats: u64, address_type: u32) -> Self {
        Self {
            recipient_address,
            amount_sats,
            address_type,
        }
    }
    pub fn to_leaf(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[0..20].copy_from_slice(&self.recipient_address);
        bytes[20..24].copy_from_slice(&self.address_type.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.amount_sats.to_le_bytes());
        bytes
    }
}

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct PsyWithdrawalChainSnapshot {
    pub auto_claimed_deposits_tree_root: QHash256,
    pub requested_withdrawals_tree_root: QHash256,
    pub block_merkle_tree_root: QHash256,
    pub block_height: u32,
    pub last_snapshotted_for_withdrawals_seconds: u32,
    pub next_requested_withdrawals_tree_index: u64,
}
impl PsyWithdrawalChainSnapshot {
    pub fn get_hash(&self) -> QHash256 {
        hash_impl_sha256_bytes(bytemuck::bytes_of(self))
    }
}

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct PsyReturnTxOutput {
    pub sighash: QHash256,
    pub output_index: u64,
    pub amount_sats: u64,
}
impl PsyReturnTxOutput {
    pub fn new(sighash: QHash256, output_index: u64, amount_sats: u64) -> Self {
        Self {
            sighash,
            output_index,
            amount_sats,
        }
    }
    pub fn get_hash(&self) -> QHash256 {
        hash_impl_sha256_bytes(bytemuck::bytes_of(self))
    }
}