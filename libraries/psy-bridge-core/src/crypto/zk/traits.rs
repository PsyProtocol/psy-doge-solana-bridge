pub type CompactBridgeZKProof = [u8; 256];
pub type CompactBridgeZKVerifierKey = [u8; 32];
pub const COMPACT_BRIDGE_ZK_PROOF_SIZE: usize = 256;
pub const COMPACT_BRIDGE_ZK_VERIFIER_KEY_SIZE: usize = 32;
pub trait ZKProofVerifier {
    type VerifierKey: Sized + Clone;
    type Proof: Sized + Clone;
    fn verify_zkp(proof: &Self::Proof, vk: &Self::VerifierKey, public_inputs: &[u8]) -> bool;
}

pub trait CompactZKProofVerifier {
    fn verify_compact_zkp(
        proof: &CompactBridgeZKProof,
        vk: &CompactBridgeZKVerifierKey,
        public_inputs: &[u8],
    ) -> bool;
    fn verify_compact_zkp_slice(
        proof: &[u8],
        vk: &[u8],
        public_inputs: &[u8],
    ) -> bool;
}

