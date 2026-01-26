//use crate::{common_types::{QHash160, QHash256}, crypto::hash::sha256_impl::hash_impl_sha256_bytes};
/*
#[cfg_attr(feature = "serialize_serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize_borsh", derive(borsh::BorshSerialize, borsh::BorshDeserialize))]
#[cfg_attr(feature = "serialize_speedy", derive(speedy::Readable, speedy::Writable))]
#[cfg_attr(feature = "serialize_bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
#[derive(PartialEq, Clone, Debug, Eq, Ord, PartialOrd, Copy, Hash, Default)]
#[repr(C)]
pub struct BridgeCustodianConfig {
    pub wallet_address_hash: QHash160,
    pub network_type: u32,
}

impl BridgeCustodianConfig {
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




#[cfg_attr(feature = "serialize_serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize_borsh", derive(borsh::BorshSerialize, borsh::BorshDeserialize))]
#[cfg_attr(feature = "serialize_speedy", derive(speedy::Readable, speedy::Writable))]
#[cfg_attr(feature = "serialize_bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
#[derive(PartialEq, Clone, Debug, Eq, Ord, PartialOrd, Copy, Hash, Default)]
#[repr(C)]
pub struct Bridge7MultisigCustodianWalletConfig {
    pub signer_public_keys: [[u8; 32]; 7],
    pub signer_public_keys_y_parity: u32,
    pub network_type: u32,
}

impl Bridge7MultisigCustodianWalletConfig {
    pub fn get_wallet_config_hash(&self) -> QHash256 {
        hash_impl_sha256_bytes(&bytemuck::bytes_of(self))
    }
    pub fn new_basic(signer_public_keys: [[u8; 32]; 7], signer_public_keys_y_parity: u32, network_type: u32) -> Self {
        Self {
            signer_public_keys,
            signer_public_keys_y_parity,
            network_type,
        }
    }
    pub fn from_compressed_public_keys(
        compressed_public_keys: [&[u8; 33]; 7],
        network_type: u32,
    ) -> Self {
        let mut public_keys: [[u8; 32]; 7] = [[0u8; 32]; 7];
        let mut signer_public_keys_y_parity = 0u32;
        for (i, compressed_key) in compressed_public_keys.iter().enumerate() {
            let y_parity = compressed_key[0];
            if y_parity == 0x03 {
                signer_public_keys_y_parity |= 1 << i;
            }
            public_keys[i].copy_from_slice(&compressed_key[1..33]);
        }
        Self {
            signer_public_keys: public_keys,
            signer_public_keys_y_parity,
            network_type,
        }
    }
    pub fn to_compressed_public_keys(&self) -> [[u8; 33]; 7] {
        let mut compressed_keys: [[u8; 33]; 7] = [[0u8; 33]; 7];
        for (i, public_key) in self.signer_public_keys.iter().enumerate() {
            let y_parity = (self.signer_public_keys_y_parity >> i) & 1;
            compressed_keys[i][0] = if y_parity == 1 { 0x03 } else { 0x02 };
            compressed_keys[i][1..33].copy_from_slice(public_key);
        }
        compressed_keys
    }
}

*/