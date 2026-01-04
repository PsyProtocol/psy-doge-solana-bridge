use std::collections::HashMap;



use crate::{common_types::{QHash160, QHash256}, crypto::secp256k1::{Secp256K1WalletProvider, signature::{CompressedPublicKey, PsyCompressedSecp256K1Signature}, single::SimpleSinglePublicKeySigner}};
use k256::ecdsa::signature::hazmat::PrehashSigner;


impl SimpleSinglePublicKeySigner<MemorySecp256K1Wallet> {
    pub fn new_insecure_memory_signer_with_private_key(private_key: QHash256) -> anyhow::Result<Self> {
        let mut wallet = MemorySecp256K1Wallet::new();
        let public_key = wallet.add_private_key(private_key)?;
        Ok(Self {
            wallet_provider: wallet,
            public_key,
        })
    }
}
#[derive(Debug, Clone)]
pub struct MemorySecp256K1Wallet {
    key_map: HashMap<CompressedPublicKey, k256::ecdsa::SigningKey>,
    p2pkh_key_map: HashMap<QHash160, CompressedPublicKey>,
}

impl Secp256K1WalletProvider for MemorySecp256K1Wallet {
    fn sign_message_hash_recoverable(
        &self,
        public_key: &CompressedPublicKey,
        message_hash: QHash256,
    ) -> anyhow::Result<(u8, [u8; 64])>{
        let private_key_result = self.key_map.get(public_key);
        if private_key_result.is_some() {
            let (signature, recovery_id) =
                private_key_result.unwrap().sign_prehash_recoverable(&message_hash)?;
            let mut rs_bytes = [0u8; 64];
            let r_bytes = signature.r().to_bytes();
            let s_bytes = signature.s().to_bytes();
            rs_bytes[0..32].copy_from_slice(&r_bytes);
            rs_bytes[32..64].copy_from_slice(&s_bytes);
            Ok((recovery_id.to_byte(), rs_bytes))
        } else {
            anyhow::bail!("private key not found")
        }
    }
    fn sign(
        &self,
        public_key: &CompressedPublicKey,
        message: QHash256,
    ) -> anyhow::Result<PsyCompressedSecp256K1Signature> {
        let private_key_result = self.key_map.get(public_key);
        if private_key_result.is_some() {
            let result: k256::ecdsa::Signature =
                private_key_result.unwrap().sign_prehash(&message)?;
            let mut rs_bytes = [0u8; 64];

            let r_bytes = result.r().to_bytes();
            let s_bytes = result.s().to_bytes();
            rs_bytes[0..32].copy_from_slice(&r_bytes);
            rs_bytes[32..64].copy_from_slice(&s_bytes);

            Ok(PsyCompressedSecp256K1Signature {
                public_key: public_key.0,
                signature: rs_bytes,
                message,
            })
        } else {
            anyhow::bail!("private key not found")
        }
    }


    fn contains_public_key(&self, public_key: &CompressedPublicKey) -> bool {
        self.key_map.contains_key(public_key)
    }

    fn get_public_keys(&self) -> Vec<CompressedPublicKey> {
        self.key_map.keys().cloned().collect()
    }

    fn contains_p2pkh_address(&self, p2pkh_address: &QHash160) -> bool {
        self.p2pkh_key_map.contains_key(p2pkh_address)
    }

    fn get_public_key_for_p2pkh(&self, p2pkh: &QHash160) -> Option<CompressedPublicKey> {
        self.p2pkh_key_map.get(p2pkh).cloned()
    }
}

impl MemorySecp256K1Wallet {
    pub fn new() -> Self {
        Self {
            key_map: HashMap::new(),
            p2pkh_key_map: HashMap::new(),
        }
    }
    pub fn add_private_key(&mut self, private_key: QHash256) -> anyhow::Result<CompressedPublicKey> {
        let signing_key = k256::ecdsa::SigningKey::from_slice(&private_key)?;
        let public_key = signing_key
            .verifying_key()
            .to_encoded_point(true)
            .to_bytes();
        let mut compressed = [0u8; 33];
        if public_key.len() == 33 {
            compressed.copy_from_slice(&public_key);
        } else {
            anyhow::bail!("public key length is not 33")
        }
        let pub_compressed = CompressedPublicKey(compressed);
        let p2pkh = pub_compressed.to_p2pkh_address();
        self.p2pkh_key_map.insert(p2pkh, pub_compressed);
        self.key_map.insert(pub_compressed, signing_key);
        Ok(pub_compressed)
    }
}


#[cfg(test)]
mod tests {
    use crate::crypto::{hash::sha256_impl::hash_impl_sha256_bytes, secp256k1::memory_wallet::MemorySecp256K1Wallet};

    #[test]
    fn check_fake_keys() {

        const TEST_PRIVATE_KEY_FAKE_ZKP_SINGLE_BLOCK: [u8; 32] = [1u8; 32];
        const TEST_PRIVATE_KEY_FAKE_ZKP_REORG: [u8; 32] = [2u8; 32];
        const TEST_PRIVATE_KEY_FAKE_ZKP_MANUAL_DEPOSIT: [u8; 32] = [3u8; 32];
        const TEST_PRIVATE_KEY_FAKE_ZKP_WITHDRAWAL: [u8; 32] = [4u8; 32];

        let mut wallet = MemorySecp256K1Wallet::new();
        let pub1 = wallet.add_private_key(TEST_PRIVATE_KEY_FAKE_ZKP_SINGLE_BLOCK).unwrap().decompress().unwrap();
        let pub2 = wallet.add_private_key(TEST_PRIVATE_KEY_FAKE_ZKP_REORG).unwrap().decompress().unwrap();
        let pub3 = wallet.add_private_key(TEST_PRIVATE_KEY_FAKE_ZKP_MANUAL_DEPOSIT).unwrap().decompress().unwrap();
        let pub4 = wallet.add_private_key(TEST_PRIVATE_KEY_FAKE_ZKP_WITHDRAWAL).unwrap().decompress().unwrap();

        let public_key_hash_single_block = hash_impl_sha256_bytes(&pub1);
        let public_key_hash_reorg = hash_impl_sha256_bytes(&pub2);
        let public_key_hash_manual_deposit = hash_impl_sha256_bytes(&pub3);
        let public_key_hash_withdrawal = hash_impl_sha256_bytes(&pub4);
        println!("const FAKE_ZKP_SINGLE_BLOCK_VK: [u8; 32] = {:?};", public_key_hash_single_block);
        println!("const FAKE_ZKP_REORG_VK: [u8; 32] = {:?};", public_key_hash_reorg);
        println!("const FAKE_ZKP_MANUAL_DEPOSIT_VK: [u8; 32] = {:?};", public_key_hash_manual_deposit);
        println!("const FAKE_ZKP_WITHDRAWAL_VK: [u8; 32] = {:?};", public_key_hash_withdrawal);

    }
}