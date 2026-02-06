use crate::{common_types::QHash256, crypto::hash::sha256_impl::hash_impl_sha256_bytes};


#[cfg_attr(
    feature = "serialize_serde",
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(
    feature = "serialize_borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize)
)]
#[cfg_attr(
    feature = "serialize_speedy",
    derive(speedy::Readable, speedy::Writable)
)]
#[cfg_attr(
    feature = "serialize_bytemuck",
    derive(bytemuck::Pod, bytemuck::Zeroable)
)]
#[derive(PartialEq, Clone, Debug, Eq, Ord, PartialOrd, Copy, Hash, Default)]
#[repr(C)]
pub struct RemoteMultisigCustodianConfig {
    pub signer_public_keys: [[u8; 32]; 7],
    pub custodian_config_id: u32,
    pub signer_public_keys_y_parity: u32,
}




#[cfg_attr(
    feature = "serialize_serde",
    derive(serde::Serialize, serde::Deserialize)
)]
#[cfg_attr(
    feature = "serialize_borsh",
    derive(borsh::BorshSerialize, borsh::BorshDeserialize)
)]
#[cfg_attr(
    feature = "serialize_speedy",
    derive(speedy::Readable, speedy::Writable)
)]
#[cfg_attr(
    feature = "serialize_bytemuck",
    derive(bytemuck::Pod, bytemuck::Zeroable)
)]
#[derive(PartialEq, Clone, Debug, Eq, Ord, PartialOrd, Copy, Hash, Default)]
#[repr(C)]
pub struct FullMultisigCustodianConfig {
    pub emitter_pubkey: [u8; 32],
    pub signer_public_keys: [[u8; 32]; 7],
    pub custodian_config_id: u32,
    pub signer_public_keys_y_parity: u16,
    pub network_type: u16,
}
// note the emitter chain is always solana which is id 1 in wormhole
// network type has an internal meaning and doesn't affect the script/address generation

/*
 5/7 multisig:
<emitter_chain>     (2 bytes, u16 BE)
<emitter_contract>  (32 bytes)
OP_2DROP
<recipient_address> (32 bytes)
OP_DROP
OP_M <pubkeys...> OP_N OP_CHECKMULTISIG
*/

// Redeem script layout (constant size = 311 bytes):
// [0]:      OP_PUSHBYTES_2
// [1-2]:    emitter_chain (2 bytes, u16 BE)
// [3]:      OP_PUSHBYTES_32
// [4-35]:   emitter_pubkey (32 bytes)
// [36]:     OP_2DROP
// [37]:     OP_PUSHBYTES_32
// [38-69]:  solana_ata (32 bytes)  <-- THIS IS WHAT WE EXTRACT
// [70]:     OP_DROP
// [71]:     OP_5
// [72-309]: 7 x (OP_PUSHBYTES_33 + 33 bytes) = 7 * 34 = 238 bytes
// [310]:    OP_7
// [311]:    OP_CHECKMULTISIG

impl FullMultisigCustodianConfig {
    pub fn from_custodian_public_keys_and_network_bridge_pda(
        network_id: u16,
        bridge_pda: [u8; 32],
        custodian_public_keys: &[[u8; 33]],
        custodian_config_id: u32,
    ) -> anyhow::Result<Self> {
        if custodian_public_keys.len() != 7 {
            return Err(anyhow::anyhow!(
                "FullMultisigCustodianConfig requires exactly 7 public keys"
            ));
        }
        let custodian_public_keys: [[u8; 33]; 7] = custodian_public_keys.try_into().unwrap();

        Self::from_compressed_public_keys(
            bridge_pda,
            custodian_public_keys,
            custodian_config_id,
            network_id,
        )
    }
    pub fn get_wallet_config_hash(&self) -> QHash256 {
        hash_impl_sha256_bytes(&bytemuck::bytes_of(self))
    }
    pub fn from_compressed_public_keys(
        emitter_pubkey: [u8; 32],
        compressed_public_keys: [[u8; 33]; 7],
        custodian_config_id: u32,
        network_type: u16,
    ) -> anyhow::Result<Self> {
        if compressed_public_keys.len() != 7 {
            return Err(anyhow::anyhow!(
                "FullMultisigCustodianConfig requires exactly 7 public keys"
            ));
        }
        let mut public_keys: [[u8; 32]; 7] = [[0u8; 32]; 7];
        let mut signer_public_keys_y_parity = 0u16;
        for (i, compressed_key) in compressed_public_keys.iter().enumerate() {
            let y_parity = compressed_key[0];
            if y_parity == 0x03 {
                signer_public_keys_y_parity |= 1 << i;
            }
            public_keys[i].copy_from_slice(&compressed_key[1..33]);
        }
        Ok(Self {
            signer_public_keys: public_keys,
            signer_public_keys_y_parity,
            custodian_config_id,
            network_type,
            emitter_pubkey,
        })
    }
    pub fn from_compressed_public_keys_refs(
        emitter_pubkey: [u8; 32],
        compressed_public_keys: &[&[u8; 33]],
        custodian_config_id: u32,
        network_type: u16,
    ) -> anyhow::Result<Self> {
        if compressed_public_keys.len() != 7 {
            return Err(anyhow::anyhow!(
                "FullMultisigCustodianConfig requires exactly 7 public keys"
            ));
        }
        let mut public_keys: [[u8; 32]; 7] = [[0u8; 32]; 7];
        let mut signer_public_keys_y_parity = 0u16;
        for (i, compressed_key) in compressed_public_keys.iter().enumerate() {
            let y_parity = compressed_key[0];
            if y_parity == 0x03 {
                signer_public_keys_y_parity |= 1 << i;
            }
            public_keys[i].copy_from_slice(&compressed_key[1..33]);
        }
        Ok(Self {
            signer_public_keys: public_keys,
            signer_public_keys_y_parity,
            custodian_config_id,
            network_type,
            emitter_pubkey,
        })
    }
    pub fn from_compressed_public_keys_buf(
        emitter_pubkey: [u8; 32],
        compressed_public_keys: &[u8],
        custodian_config_id: u32,
        network_type: u16,
    ) -> anyhow::Result<Self> {
        if compressed_public_keys.len() != 7 * 33{
            return Err(anyhow::anyhow!(
                "FullMultisigCustodianConfig requires exactly 7 public keys"
            ));
        }
        let mut public_keys: [[u8; 32]; 7] = [[0u8; 32]; 7];
        let mut signer_public_keys_y_parity = 0u16;
        for i in 0..7 {
            let start_index = i * 33;
            let y_parity = compressed_public_keys[start_index];
            if y_parity == 0x03 {
                signer_public_keys_y_parity |= 1 << i;
            }
            public_keys[i].copy_from_slice(&compressed_public_keys[start_index + 1..start_index + 33]);
        }
        Ok(Self {
            signer_public_keys: public_keys,
            signer_public_keys_y_parity,
            custodian_config_id,
            network_type,
            emitter_pubkey,
        })
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