use psy_bridge_core::crypto::{
    hash::sha256_impl::hash_impl_sha256_bytes,
    secp256k1::{
        memory_wallet::MemorySecp256K1Wallet, single::SimpleSinglePublicKeySigner,
        Secp256K1WalletProvider, SimpleSingleSigner,
    },
    zk::jtmb::FakeZKProof,
};

const TEST_PRIVATE_KEY_FAKE_ZKP_SINGLE_BLOCK: [u8; 32] = [1u8; 32];
const TEST_PRIVATE_KEY_FAKE_ZKP_REORG: [u8; 32] = [2u8; 32];
const TEST_PRIVATE_KEY_FAKE_ZKP_MANUAL_DEPOSIT: [u8; 32] = [3u8; 32];
const TEST_PRIVATE_KEY_FAKE_ZKP_WITHDRAWAL: [u8; 32] = [4u8; 32];

pub struct FakeZKProofKeyPair {
    pub private_key: [u8; 32],
    pub public_key: [u8; 64],
    pub vk: [u8; 32],
}

impl FakeZKProofKeyPair {
    pub fn new(private_key: [u8; 32], public_key: [u8; 64], vk: [u8; 32]) -> Self {
        Self {
            private_key,
            public_key,
            vk,
        }
    }
    pub fn new_from_private_key(private_key: [u8; 32]) -> anyhow::Result<Self> {
        let mem =
            SimpleSinglePublicKeySigner::new_insecure_memory_signer_with_private_key(private_key)?;
        let public_key = mem.get_compressed_public_key().decompress()?;
        let vk = hash_impl_sha256_bytes(&public_key);
        Ok(Self {
            private_key,
            public_key,
            vk,
        })
    }
    pub fn generate_fake_zkp(&self, public_inputs_hash: [u8; 32]) -> anyhow::Result<FakeZKProof> {
        let mut signer = MemorySecp256K1Wallet::new();
        let compressed_pk = signer.add_private_key(self.private_key)?;

        let (recovery_id, signature) =
            signer.sign_message_hash_recoverable(&compressed_pk, public_inputs_hash)?;

        Ok(FakeZKProof {
            signature,
            public_key: self.public_key,
            recovery_id,
            _padding: [0u8; 7],
        })
    }
}
pub struct FakeZKProofGenerator {
    pub single_block: FakeZKProofKeyPair,
    pub reorg: FakeZKProofKeyPair,
    pub manual_deposit: FakeZKProofKeyPair,
    pub withdrawal: FakeZKProofKeyPair,
}

impl FakeZKProofGenerator {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            single_block: FakeZKProofKeyPair::new_from_private_key(
                TEST_PRIVATE_KEY_FAKE_ZKP_SINGLE_BLOCK,
            )?,
            reorg: FakeZKProofKeyPair::new_from_private_key(TEST_PRIVATE_KEY_FAKE_ZKP_REORG)?,
            manual_deposit: FakeZKProofKeyPair::new_from_private_key(
                TEST_PRIVATE_KEY_FAKE_ZKP_MANUAL_DEPOSIT,
            )?,
            withdrawal: FakeZKProofKeyPair::new_from_private_key(
                TEST_PRIVATE_KEY_FAKE_ZKP_WITHDRAWAL,
            )?,
        })
    }
}
