use psy_bridge_core::{common_types::QHash256, crypto::{hash::sha256_impl::hash_impl_sha256_bytes, zk::CompactZKProofVerifier}, error::{DogeBridgeError, QDogeResult}};

use crate::{
    generic_cpi::BurnCPIHelper,
    program_state::{PsyBridgeProgramState, PsyReturnTxOutput, PsyWithdrawalRequest},
};


impl PsyBridgeProgramState {
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

        let doge_tx_hash = hash_impl_sha256_bytes(dogecoin_tx);
        if doge_tx_hash != new_return_output.sighash {
            return Err(DogeBridgeError::InvalidDogeTxHash);
        }
        let expected_public_inputs = self.get_expected_public_inputs_for_withdrawal_proof(
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
    ) -> QDogeResult<()> {
        let ok = self.process_request_withdrawal(
            request.address_type,
            request.recipient_address,
            request.amount_sats
        );

        if !ok {
            return Err(DogeBridgeError::InvalidWithdrawalAmount);

        }

        burner.burn_from(requester, request.amount_sats)?;

        Ok(())
        
    }
}
