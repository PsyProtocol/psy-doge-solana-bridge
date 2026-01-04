use crate::{common_types::QHash256, crypto::secp256k1::{Secp256K1WalletProvider, SimpleSingleSigner, signature::{CompressedPublicKey, PsyCompressedSecp256K1Signature}}};


pub struct SimpleSinglePublicKeySigner<T: Secp256K1WalletProvider> {
    pub wallet_provider: T,
    pub public_key: CompressedPublicKey,
}
impl<T: Secp256K1WalletProvider> SimpleSingleSigner for SimpleSinglePublicKeySigner<T> {
    fn sign_message(&self, message: QHash256) -> anyhow::Result<PsyCompressedSecp256K1Signature> {
        self.wallet_provider.sign(&self.public_key, message)
    }
    
    fn get_compressed_public_key(&self) -> CompressedPublicKey {
        self.public_key
    }
}