pub fn secp256k1_recover_uncompressed(prehash: &[u8], recovery_id: u8, signature: &[u8]) -> anyhow::Result<[u8; 64]> {
    if prehash.len() != 32 {
        anyhow::bail!("prehash length is not 32");
    }
    if signature.len() != 64 {
        anyhow::bail!("signature length is not 64");
    }
    #[cfg(all(feature = "secp256k1", not(feature = "solprogram")))]
    {
        use k256::ecdsa::{Signature, RecoveryId};
        if let Some(rec_id) = RecoveryId::from_byte(recovery_id) {
            use k256::ecdsa::VerifyingKey;

            let signature= Signature::from_bytes(signature.into())?;
            let verifying_key = VerifyingKey::recover_from_prehash(prehash, &signature, rec_id)?;
            
            let uncompressed_pubkey = verifying_key.to_encoded_point(false);
            let pubkey_bytes = uncompressed_pubkey.as_bytes();
            let mut uncompressed = [0u8; 64];
            if pubkey_bytes.len() == 65 && pubkey_bytes[0] == 0x04 {
                uncompressed.copy_from_slice(&pubkey_bytes[1..65]);
                Ok(uncompressed)
            } else {
                anyhow::bail!("public key length is not 64")
            }
        } else {
            anyhow::bail!("invalid recovery id")
        }
    }
    #[cfg(all(feature = "solprogram"))]
    {
        let public_key = solana_program::secp256k1_recover::secp256k1_recover(&prehash, recovery_id, &signature)
            .map_err(|_| anyhow::anyhow!("secp256k1 recovery failed"))?;
        Ok(public_key.0)
    }
    #[cfg(all(not(feature = "secp256k1"), not(feature = "solprogram")))]
    {
        anyhow::bail!("secp256k1 recovery not supported")
    }
}