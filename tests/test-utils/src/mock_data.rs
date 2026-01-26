
use psy_bridge_core::{
    crypto::{
        hash::sha256_impl::hash_impl_sha256_bytes, zk::CompactBridgeZKProof
    }, header::{PsyBridgeHeader, PsyBridgeStateCommitment, PsyBridgeTipStateCommitment}
};
use psy_doge_solana_core::{data_accounts::pending_mint::{PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH, PM_TXO_DEFAULT_BUFFER_HASH, PendingMint}, fake_zkp::FakeZKProofGenerator};

pub fn generate_fake_header(height: u32) -> PsyBridgeHeader {
    let empty_hash = [0u8; 32];
    let state = PsyBridgeStateCommitment {
        block_hash: empty_hash,
        block_merkle_tree_root: empty_hash,
        pending_mints_finalized_hash: PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH,
        txo_output_list_finalized_hash: PM_TXO_DEFAULT_BUFFER_HASH, 
        auto_claimed_txo_tree_root: empty_hash,
        auto_claimed_deposits_tree_root: empty_hash,
        auto_claimed_deposits_next_index: 0,
        block_height: height,
    };
    
    PsyBridgeHeader {
        tip_state: PsyBridgeTipStateCommitment {
            block_hash: empty_hash,
            block_merkle_tree_root: empty_hash,
            block_time: 0,
            block_height: height,
        },
        finalized_state: state,
        bridge_state_hash: empty_hash,
        last_rollback_at_secs: 0,
        paused_until_secs: 0,
        total_finalized_fees_collected_chain_history: 0,
    }
}


pub fn generate_block_update_fake_proof(public_inputs: [u8; 32]) -> CompactBridgeZKProof {
    let generator = FakeZKProofGenerator::new().unwrap();
    generator.single_block.generate_fake_zkp(public_inputs).unwrap().to_compact_zkp()
}

pub fn generate_block_update_reorg_fake_proof(public_inputs: [u8; 32]) -> CompactBridgeZKProof {
    let generator = FakeZKProofGenerator::new().unwrap();
    generator.reorg.generate_fake_zkp(public_inputs).unwrap().to_compact_zkp()
}

pub fn generate_withdrawal_fake_proof(public_inputs: [u8; 32]) -> CompactBridgeZKProof {
    let generator = FakeZKProofGenerator::new().unwrap();
    generator.withdrawal.generate_fake_zkp(public_inputs).unwrap().to_compact_zkp()
}

pub fn generate_manual_claim_fake_proof(public_inputs: [u8; 32]) -> CompactBridgeZKProof {
    let generator = FakeZKProofGenerator::new().unwrap();
    generator.manual_deposit.generate_fake_zkp(public_inputs).unwrap().to_compact_zkp()
}

pub fn compute_pending_mints_hash(mints: &[PendingMint]) -> [u8; 32] {
    if mints.is_empty() {
        return PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH;
    }

    let count = mints.len() as u16;
    let group_size = 24;
    let num_groups = (mints.len() + group_size - 1) / group_size;
    
    let mut preimage = Vec::new();
    preimage.extend_from_slice(&count.to_le_bytes());
    
    for i in 0..num_groups {
        let start = i * group_size;
        let end = std::cmp::min(start + group_size, mints.len());
        
        let mut group_bytes = Vec::new();
        for m in &mints[start..end] {
            group_bytes.extend_from_slice(bytemuck::bytes_of(m));
        }
        let group_hash = hash_impl_sha256_bytes(&group_bytes);
        preimage.extend_from_slice(&group_hash);
    }
    
    hash_impl_sha256_bytes(&preimage)
}