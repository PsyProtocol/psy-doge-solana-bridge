// For local development to speed up proving we can use a JTMB ZK prover that just returns an empty proof.

use crate::{common_types::QHash256, crypto::{
    hash::sha256_impl::hash_impl_sha256_bytes, secp256k1::recover::secp256k1_recover_uncompressed, zk::{
        CompactBridgeZKProof, CompactBridgeZKVerifierKey, CompactZKProofVerifier, ZKProofVerifier,
    }
}};

#[macro_rules_attribute::apply(crate::DeriveCopySerializeReprC)]
pub struct FakeZKProof {
    #[cfg_attr(
        feature = "serialize_serde",
        serde(with = "crate::serde_arrays::serde_arrays")
    )]
    pub signature: [u8; 64],
    #[cfg_attr(
        feature = "serialize_serde",
        serde(with = "crate::serde_arrays::serde_arrays")
    )]
    pub public_key: [u8; 64],
    pub recovery_id: u8,
    pub _padding: [u8; 7], // Ensure 8-byte alignment for the struct
}
impl FakeZKProof {
    pub fn new(signature: [u8; 64], public_key: [u8; 64], recovery_id: u8) -> Self {
        Self {
            signature,
            public_key,
            recovery_id,
            _padding: [0u8; 7],
        }
    }
    pub fn to_compact_zkp(&self) -> CompactBridgeZKProof {
        let mut fake_zkp_bytes = [0u8; 256];
        fake_zkp_bytes[0..64].copy_from_slice(&self.signature);
        fake_zkp_bytes[64..128].copy_from_slice(&self.public_key);
        fake_zkp_bytes[128..192].copy_from_slice(&self.signature);
        fake_zkp_bytes[192..256].copy_from_slice(&self.public_key);
        fake_zkp_bytes[255] = self.recovery_id;
        fake_zkp_bytes
    }
    pub fn check_compact_zkp_consistent(fake_zkp_bytes: &[u8]) -> bool {
        if fake_zkp_bytes.len() != 256 {
            return false;
        }
        for i in 0..127 {
            if fake_zkp_bytes[i] != fake_zkp_bytes[128 + i] {
                return false;
            }
        }
        true
    }
}

#[macro_rules_attribute::apply(crate::DeriveCopySerializeReprC)]
pub struct FakeZKVerifierData {
    pub public_key_hash: QHash256,
}
impl FakeZKVerifierData {
    pub fn new(public_key_hash: QHash256) -> Self {
        Self { public_key_hash }
    }
    pub fn new_from_public_key(public_key: &[u8; 64]) -> Self {
        let public_key_hash = hash_impl_sha256_bytes(&public_key[..]);
        Self {
            public_key_hash,
        }
    }
    pub fn to_compact_vk(&self) -> CompactBridgeZKVerifierKey {
        self.public_key_hash
    }
    pub fn check_compact_vk_consistent(fake_zkp_bytes: &[u8]) -> bool {
        if fake_zkp_bytes.len() != 32 {
            return false;
        }
        true
    }
}
impl ZKProofVerifier for FakeZKProof {
    type VerifierKey = FakeZKVerifierData;
    type Proof = FakeZKProof;

    fn verify_zkp(proof: &FakeZKProof, vk: &FakeZKVerifierData, public_inputs: &[u8]) -> bool {
        if public_inputs.len() != 32 {
            return false;
        }
        match secp256k1_recover_uncompressed(&public_inputs, proof.recovery_id, &proof.signature) {
            Ok(recovered_public_key) => hash_impl_sha256_bytes(&recovered_public_key) == vk.public_key_hash,
            Err(_) => false,
        }
    }
}

impl CompactZKProofVerifier for FakeZKProof {
    fn verify_compact_zkp(
        proof: &CompactBridgeZKProof,
        vk: &CompactBridgeZKVerifierKey,
        public_inputs: &[u8],
    ) -> bool {
        Self::verify_compact_zkp_slice(proof, vk, public_inputs)
    }
    fn verify_compact_zkp_slice(proof: &[u8], vk: &[u8], public_inputs: &[u8]) -> bool {
        if public_inputs.len() != 32 {
            return false;
        }
        if vk.len() != 32 {
            return false;
        }
        if proof.len() != 256 {
            return false;
        }
        if !FakeZKProof::check_compact_zkp_consistent(proof)
            || !FakeZKVerifierData::check_compact_vk_consistent(vk)
        {
            return false;
        }
        let recovery_id = proof[255];
        let signature_ptr = &proof[0..64];
        match secp256k1_recover_uncompressed(&public_inputs, recovery_id, signature_ptr) {
            Ok(recovered_public_key) => hash_impl_sha256_bytes(&recovered_public_key) == vk,
            Err(_) => false,
        }
    }
}
