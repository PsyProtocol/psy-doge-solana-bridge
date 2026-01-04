use crate::common_types::{QHash160, QHash256};
use crate::crypto::hash::ripemd160_impl::hash_impl_btc_hash160_bytes;

pub fn u256_to_der(u256: &[u8]) -> Vec<u8> {
    assert_eq!(u256.len(), 32);
    let mut result = vec![];
    result.push(0x02u8);
    if (u256[0] & 0x80) != 0 {
        result.push((u256.len() + 1) as u8);
        result.push(0);
        result.extend_from_slice(u256);
    } else {
        result.push(u256.len() as u8);
        result.extend_from_slice(u256);
    }
    result
}

#[cfg_attr(feature = "serialize_serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize_borsh", derive(borsh::BorshSerialize, borsh::BorshDeserialize))]
#[cfg_attr(feature = "serialize_speedy", derive(speedy::Readable, speedy::Writable))]
#[derive(PartialEq, Clone, Copy, Debug, Hash, Eq, Ord, PartialOrd)]
pub struct PsyCompressedSecp256K1Signature {
    #[cfg_attr(feature = "serialize_serde", serde(with = "crate::serde_arrays::serde_arrays"))]
    pub public_key: [u8; 33],

    #[cfg_attr(feature = "serialize_serde", serde(with = "crate::serde_arrays::serde_arrays"))]
    pub signature: [u8; 64],

    pub message: QHash256,
}
impl PsyCompressedSecp256K1Signature {
    pub fn to_btc_script(&self) -> Vec<u8> {
        let r = u256_to_der(&self.signature[0..32]);
        let s = u256_to_der(&self.signature[32..64]);
        let combined_rs_length = (r.len() + s.len()) as u8;
        let sig_stack_raw = [
            vec![combined_rs_length + 3, 0x30u8, combined_rs_length],
            r,
            s,
            vec![0x01],
        ]
        .concat();
        [sig_stack_raw, vec![0x21], self.public_key.to_vec()].concat()
    }

    #[cfg(feature = "secp256k1")]
    pub fn to_uncompressed_signature(&self) -> anyhow::Result<PsySecp256K1Signature> {
        use k256::{Secp256k1, ecdsa::VerifyingKey, elliptic_curve::sec1::EncodedPoint};
        let verifying_key = EncodedPoint::<Secp256k1>::from_bytes(&self.public_key)?;
        let uncompressed_pubkey_bytes = VerifyingKey::from_encoded_point(&verifying_key)?
            .to_encoded_point(false)
            .to_bytes();
        let mut uncompressed = [0u8; 64];
        if uncompressed_pubkey_bytes.len() == 65 && uncompressed_pubkey_bytes[0] == 0x04 {
            uncompressed.copy_from_slice(&uncompressed_pubkey_bytes[1..65]);
        } else {
            anyhow::bail!("public key length is not 64")
        }
        Ok(PsySecp256K1Signature {
            public_key: uncompressed,
            signature: self.signature,
            message: self.message,
        })
    }
    pub fn verify(&self) -> anyhow::Result<()> {
        #[cfg(feature = "secp256k1")]
        {
            use k256::{Secp256k1, ecdsa::{VerifyingKey, signature::hazmat::PrehashVerifier}, elliptic_curve::sec1::EncodedPoint};
            let enc_point = EncodedPoint::<Secp256k1>::from_bytes(&self.public_key)?;
            let verifying_key = VerifyingKey::from_encoded_point(&enc_point)?;
            let signature = k256::ecdsa::Signature::from_bytes(&self.signature.into())?;
            verifying_key.verify_prehash(&self.message, &signature)?;
            Ok(())
        }
        #[cfg(not(feature = "secp256k1"))]
        {
            anyhow::bail!("secp256k1 feature not enabled")
        }
    }
}

#[macro_rules_attribute::apply(crate::DeriveCopySerializeReprC)]
pub struct PsySecp256K1Signature {
    #[cfg_attr(feature = "serialize_serde", serde(with = "crate::serde_arrays::serde_arrays"))]
    pub public_key: [u8; 64],

    #[cfg_attr(feature = "serialize_serde", serde(with = "crate::serde_arrays::serde_arrays"))]
    pub signature: [u8; 64],

    pub message: QHash256,
}

impl PsySecp256K1Signature {
    #[cfg(feature = "secp256k1")]
    pub fn to_compressed_signature(&self) -> anyhow::Result<PsyCompressedSecp256K1Signature> {
        use k256::ecdsa::VerifyingKey;
        let verifying_key = VerifyingKey::from_sec1_bytes(&self.public_key)?;
        let compressed_pubkey_bytes = verifying_key.to_encoded_point(true).to_bytes();
        let mut compressed = [0u8; 33];
        if compressed_pubkey_bytes.len() == 33 {
            compressed.copy_from_slice(&compressed_pubkey_bytes);
        } else {
            anyhow::bail!("public key length is not 33")
        }
        Ok(PsyCompressedSecp256K1Signature {
            public_key: compressed,
            signature: self.signature,
            message: self.message,
        })
    }
    pub fn verify(&self) -> anyhow::Result<()> {
        #[cfg(feature = "secp256k1")]
        {
            use k256::ecdsa::{VerifyingKey, signature::hazmat::PrehashVerifier};
            let verifying_key = VerifyingKey::from_sec1_bytes(&self.public_key)?;
            let signature = k256::ecdsa::Signature::from_bytes(&self.signature.into())?;
            verifying_key.verify_prehash(&self.message, &signature)?;
            Ok(())
        }
        #[cfg(not(feature = "secp256k1"))]
        {
            anyhow::bail!("secp256k1 feature not enabled")
        }
    }
}

#[cfg_attr(feature = "serialize_serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serialize_borsh", derive(borsh::BorshSerialize, borsh::BorshDeserialize))]
#[cfg_attr(feature = "serialize_speedy", derive(speedy::Readable, speedy::Writable))]
#[derive(PartialEq, Clone, Copy, Debug, Hash, Eq, Ord, PartialOrd)]
#[repr(transparent)]
pub struct CompressedPublicKey(
    #[cfg_attr(feature = "serialize_serde", serde(with = "crate::serde_arrays::serde_arrays"))]
    pub [u8; 33]
);
impl CompressedPublicKey {
    pub fn to_p2pkh_address(&self) -> QHash160 {
        hash_impl_btc_hash160_bytes(&self.0)
    }
    pub fn decompress(&self) -> anyhow::Result<[u8; 64]> {
        #[cfg(feature = "secp256k1")]
        {
            use k256::{Secp256k1, ecdsa::VerifyingKey, elliptic_curve::sec1::EncodedPoint};
            let verifying_key = EncodedPoint::<Secp256k1>::from_bytes(&self.0)?;
            let uncompressed_pubkey_bytes = VerifyingKey::from_encoded_point(&verifying_key)?
                .to_encoded_point(false)
                .to_bytes();
            let mut uncompressed = [0u8; 64];
            if uncompressed_pubkey_bytes.len() == 65 && uncompressed_pubkey_bytes[0] == 0x04 {
                uncompressed.copy_from_slice(&uncompressed_pubkey_bytes[1..65]);
                Ok(uncompressed)
            } else {
                anyhow::bail!("public key length is not 64")
            }
        }
        #[cfg(not(feature = "secp256k1"))]
        {
            anyhow::bail!("secp256k1 feature not enabled")
        }
    }
}