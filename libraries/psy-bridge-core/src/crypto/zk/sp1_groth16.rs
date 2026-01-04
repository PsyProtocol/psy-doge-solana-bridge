pub use sp1_solana::{verify_proof_raw, GROTH16_VK_5_0_0_BYTES};

use crate::crypto::{hash::sha256_impl::hash_impl_sha256_bytes, zk::{CompactZKProofVerifier, ZKProofVerifier}};

/// Hashes the public inputs in the same format as the Groth16 verifier.
fn hash_public_inputs(public_inputs: &[u8]) -> [u8; 32] {
    let mut result = hash_impl_sha256_bytes(public_inputs);

    // The Groth16 verifier operates over a 254 bit field (BN254), so we need to zero
    // out the first 3 bits. The same logic happens in the SP1 Ethereum verifier contract.
    result[0] &= 0x1F;

    result
}

/// Formats the sp1 vkey hash and public inputs for use in the Groth16 verifier.
fn groth16_public_values(sp1_vkey_hash: &[u8; 32], sp1_public_inputs: &[u8]) -> [u8; 63] {
    let mut result = [0u8; 63];
    result[0..31].copy_from_slice(&sp1_vkey_hash[1..32]);
    let committed_values_digest = hash_public_inputs(sp1_public_inputs);
    result[31..63].copy_from_slice(&committed_values_digest);

    result
}

pub struct SP1Groth16Verifier;
impl ZKProofVerifier for SP1Groth16Verifier {
    type VerifierKey = [u8; 32];
    type Proof = [u8; 256];

    fn verify_zkp(
        proof: &Self::Proof,
        vk: &Self::VerifierKey,
        public_inputs: &[u8],
    ) -> bool {
        let groth16_public_inputs = groth16_public_values(vk, public_inputs);
        match verify_proof_raw(proof, &groth16_public_inputs, &GROTH16_VK_5_0_0_BYTES) {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}

impl CompactZKProofVerifier for SP1Groth16Verifier {
    fn verify_compact_zkp(
        proof: &[u8; 256],
        vk: &[u8; 32],
        public_inputs: &[u8],
    ) -> bool {
        if proof.len() != 256 || vk.len() != 32 {
            return false;
        }
        let mut proof_array = [0u8; 256];
        proof_array.copy_from_slice(proof);
        let mut vk_array = [0u8; 32];
        vk_array.copy_from_slice(vk);
        Self::verify_zkp(&proof_array, &vk_array, public_inputs)
    }
    
    fn verify_compact_zkp_slice(
        proof: &[u8],
        vk: &[u8],
        public_inputs: &[u8],
    ) -> bool {
        if proof.len() != 256 || vk.len() != 32 || public_inputs.len() != 32{
            return false;
        }
        let groth16_public_inputs = groth16_public_values(
            vk.try_into().unwrap(),
            public_inputs,
        );
        match verify_proof_raw(proof, &groth16_public_inputs, &GROTH16_VK_5_0_0_BYTES) {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}