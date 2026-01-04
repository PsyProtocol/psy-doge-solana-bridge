use psy_bridge_core::{common_types::QHash256, crypto::{hash::sha256::btc_hash256_bytes, zk::CompactZKProofVerifier}, error::{DogeBridgeError, QDogeResult}};

use crate::{
    generic_cpi::BurnCPIHelper,
    program_state::{PsyBridgeProgramState, PsyReturnTxOutput, PsyWithdrawalRequest},
};


impl PsyBridgeProgramState {
    // only callable from the user manual mint program which checks a zkp to ensure that the user's deposit is not in the auto claimed tree and not in the user's own manually claimed tree
    // we need to check to make sure the caller/signer's PDA corresponds to the correct user manual mint program and the user and the first valid seed (ie. ensure you cannot have multiple instances per user)
    pub fn run_process_bridge_withdrawal<
        ZKVerfier: CompactZKProofVerifier    
        >(
        &mut self,
        proof: &[u8],
        vk: &[u8],
        dogecoin_tx: &[u8],
        new_return_output: PsyReturnTxOutput,
        new_spent_txo_tree_root: QHash256,
        new_next_processed_withdrawals_index: u64,
    ) -> QDogeResult<QHash256> {

        let doge_tx_hash = btc_hash256_bytes(dogecoin_tx);
        let expected_public_inputs = self.get_expected_public_inputs_for_withdrawal_proof(
            &doge_tx_hash,
            &new_return_output,
            new_spent_txo_tree_root,
            new_next_processed_withdrawals_index,
        );

        let is_zkp_valid = ZKVerfier::verify_compact_zkp_slice(proof, vk, &expected_public_inputs);
        if !is_zkp_valid {
            return Err(DogeBridgeError::BridgeZKPError);
        }

        self.update_for_withdrawal(new_return_output, new_spent_txo_tree_root, new_next_processed_withdrawals_index);

        
        Ok(doge_tx_hash)
    }

    pub fn request_withdrawal<Burner: BurnCPIHelper>(
        &mut self,
        burner: &Burner,
        requester: &[u8; 32],
        request: &PsyWithdrawalRequest,
        recipient_address: [u8; 20],
        address_type: u32, // 0 = P2PKH, 1 = P2SH
        amount_sats: u64,
    ) -> QDogeResult<()> {
        let ok = self.process_request_withdrawal(
            address_type,
            recipient_address,
            amount_sats
        );

        if !ok {
            return Err(DogeBridgeError::InvalidWithdrawalAmount);

        }

        burner.burn_from(requester, request.amount_sats)?;

        Ok(())
        
    }
}
