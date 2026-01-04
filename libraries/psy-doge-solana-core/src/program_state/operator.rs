use psy_bridge_core::
    error::{DogeBridgeError, QDogeResult}
;

use crate::{
    generic_cpi::MintCPIHelper,
    program_state::PsyBridgeProgramState,
};


impl PsyBridgeProgramState {
    // Modified to return the fee amount instead of executing mint, to allow separating state mutation from CPI
    pub fn run_bridge_operator_withdraw_fees_precheck(
        &mut self,
    ) -> QDogeResult<u64> {
        if self.get_total_finalized_fees() <= self.total_fees_withdrawn_sats {
            return Err(DogeBridgeError::NoOperatorFeesToWithdraw);
        }
        let fees_to_withdraw = self.get_total_finalized_fees() - self.total_fees_withdrawn_sats;
        if fees_to_withdraw == 0 {
            // sanity check
            return Err(DogeBridgeError::NoOperatorFeesToWithdraw);
        }
        self.total_fees_withdrawn_sats += fees_to_withdraw;
        
        Ok(fees_to_withdraw)
    }

    pub fn run_bridge_operator_withdraw_fees<Minter: MintCPIHelper>(
        &mut self,
        minter: &Minter,
        operator_ata: &[u8; 32],
    ) -> QDogeResult<()> {
        let fees_to_withdraw = self.run_bridge_operator_withdraw_fees_precheck()?;
        minter.mint_to(0, operator_ata, fees_to_withdraw)?;
        Ok(())
    }
}