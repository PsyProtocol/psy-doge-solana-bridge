use bytemuck::{Pod, Zeroable};
use psy_bridge_core::{common_types::QHash256, crypto::zk::CompactZKProofVerifier, error::{DogeBridgeError, QDogeResult}, txo_constants::TXO_EMPTY_MERKLE_TREE_ROOT};

use crate::{generic_cpi::ManualDepositMainBridgeCPIHelper, public_inputs::get_manual_deposit_proof_public_inputs};

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug, Pod, Zeroable)]
#[repr(transparent)]
pub struct UserManualDepositManagerProgramState {
    pub manual_claimed_txo_tree_root: [u8; 32],
}

impl UserManualDepositManagerProgramState {
    pub fn new_empty() -> Self {
        // in the program we need to ensure we derive A SINGLE PDA account for a given signer public key.
        // this ensures
        Self {
            manual_claimed_txo_tree_root: TXO_EMPTY_MERKLE_TREE_ROOT,
        }
    }

    pub fn manual_claim_deposit<
        ZKVerifier: CompactZKProofVerifier,
        BridgeManualDepositHelper: ManualDepositMainBridgeCPIHelper,
    >(
        &mut self,
        proof: &[u8],
        known_manual_claim_deposit_vk: &[u8],
        manual_claim_helper: &BridgeManualDepositHelper,
        recent_block_merkle_tree_root: QHash256,
        recent_auto_claim_txo_root: QHash256,
        new_manual_claim_txo_root: QHash256,
        tx_hash: QHash256,
        combined_txo_index: u64,
        signer_public_key: [u8; 32],
        deposit_amount_sats: u64,
    ) -> QDogeResult<()> {
        if !manual_claim_helper.ensure_current_program_is_lowest_possible_pda_seed(signer_public_key) {
            return Err(DogeBridgeError::InvalidPDAForManualDepositManager);
        }
        let token_pda = manual_claim_helper.derive_token_ata_from_signer(signer_public_key);

        let expected_public_inputs = get_manual_deposit_proof_public_inputs(
            &recent_block_merkle_tree_root,
            &recent_auto_claim_txo_root,
            &self.manual_claimed_txo_tree_root,
            &new_manual_claim_txo_root,
            &tx_hash,
            &token_pda,
            combined_txo_index,
            deposit_amount_sats,
        );

        if !ZKVerifier::verify_compact_zkp_slice(proof, known_manual_claim_deposit_vk, &expected_public_inputs) {
            return Err(DogeBridgeError::BridgeZKPError);
        }

        self.manual_claimed_txo_tree_root = new_manual_claim_txo_root;
        manual_claim_helper.process_manual_deposit(
            recent_block_merkle_tree_root,
            recent_auto_claim_txo_root,
            tx_hash,
            combined_txo_index,
            signer_public_key,
            deposit_amount_sats,
        )?;
        Ok(())
    }
}