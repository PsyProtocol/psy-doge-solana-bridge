use crate::{common_types::{QHash160, QHash256}, crypto::secp256k1::signature::{CompressedPublicKey, PsyCompressedSecp256K1Signature}};

pub trait Secp256K1WalletProvider {
    fn sign(
        &self,
        public_key: &CompressedPublicKey,
        message: QHash256,
    ) -> anyhow::Result<PsyCompressedSecp256K1Signature>;
    fn contains_public_key(&self, public_key: &CompressedPublicKey) -> bool;
    fn contains_p2pkh_address(&self, p2pkh_address: &QHash160) -> bool;
    fn get_public_key_for_p2pkh(&self, p2pkh: &QHash160) -> Option<CompressedPublicKey>;
    fn get_public_keys(&self) -> Vec<CompressedPublicKey>;
    fn sign_message_hash_recoverable(
        &self,
        public_key: &CompressedPublicKey,
        message_hash: QHash256,
    ) -> anyhow::Result<(u8, [u8; 64])>;
}

pub trait SimpleSingleSigner {
    fn sign_message(&self, message: QHash256) -> anyhow::Result<PsyCompressedSecp256K1Signature>;
    fn get_compressed_public_key(&self) -> CompressedPublicKey;
}