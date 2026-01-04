use psy_bridge_core::{common_types::QHash256, error::QDogeResult};

use crate::{
    generic_cpi::MintCPIHelper,
    program_state::PsyBridgeProgramState,
};


impl PsyBridgeProgramState {
    // only callable from the user manual mint program which checks a zkp to ensure that the user's deposit is not in the auto claimed tree and not in the user's own manually claimed tree
    // we need to check to make sure the caller/signer's PDA corresponds to the correct user manual mint program and the user and the first valid seed (ie. ensure you cannot have multiple instances per user)
    pub fn run_claim_deposit_manual<Minter: MintCPIHelper>(
        &mut self,
        minter: &Minter,
        recent_block_merkle_tree_root: QHash256,
        recent_auto_claim_txo_root: QHash256,
        tx_hash: QHash256,
        combined_txo_index: u64,
        depositor_public_key: &[u8; 32],
        deposit_amount_sats: u64,
    ) -> QDogeResult<()> {

        let mint_amount = self.process_manual_claimed_deposit(tx_hash, recent_block_merkle_tree_root, recent_auto_claim_txo_root, combined_txo_index, depositor_public_key, deposit_amount_sats)?;
        minter.mint_to(0, depositor_public_key, mint_amount)?;
        Ok(())
    }
}
