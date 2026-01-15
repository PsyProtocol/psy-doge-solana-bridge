use crate::{common_types::{QHash160, QHash256}, crypto::hash::sha256_impl::hash_impl_sha256_bytes};

#[cfg_attr(feature = "serialize_serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize_borsh", derive(borsh::BorshSerialize, borsh::BorshDeserialize))]
#[cfg_attr(feature = "serialize_speedy", derive(speedy::Readable, speedy::Writable))]
#[cfg_attr(feature = "serialize_bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
#[derive(PartialEq, Clone, Debug, Eq, Ord, PartialOrd, Copy, Hash, Default)]
#[repr(C)]
pub struct BridgeCustodianWalletConfig {
    pub wallet_address_hash: QHash160,
    pub network_type: u32,
}

impl BridgeCustodianWalletConfig {
    pub fn get_wallet_config_hash(&self) -> QHash256 {
        hash_impl_sha256_bytes(&bytemuck::bytes_of(self))
    }
    pub fn new_basic(wallet_address_hash: QHash160, network_type: u32) -> Self {
        Self {
            wallet_address_hash,
            network_type,
        }
    }
}

