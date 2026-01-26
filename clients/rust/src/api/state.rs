//! State query implementations.

use crate::{
    client::BridgeClient,
    errors::BridgeError,
    types::{PsyBridgeProgramState, PsyWithdrawalChainSnapshot},
};
use psy_doge_solana_core::program_state::BridgeProgramStateWithDogeMint;
use solana_sdk::commitment_config::CommitmentConfig;

impl BridgeClient {
    /// Get the current bridge program state from on-chain.
    pub async fn get_current_bridge_state_impl(&self) -> Result<PsyBridgeProgramState, BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        self.retry_executor
            .execute(|| async {
                let account = self
                    .rpc
                    .get_account_with_commitment(
                        &self.config.bridge_state_pda,
                        CommitmentConfig::confirmed(),
                    )
                    .await?
                    .value
                    .ok_or_else(|| BridgeError::AccountNotFound {
                        address: self.config.bridge_state_pda.to_string(),
                    })?;

                let bridge_state: &BridgeProgramStateWithDogeMint =
                    bytemuck::from_bytes(&account.data);

                Ok(bridge_state.core_state.clone())
            })
            .await
    }

    /// Get the DOGE mint from on-chain state.
    pub async fn get_doge_mint_from_state(&self) -> Result<solana_sdk::pubkey::Pubkey, BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let account = self
            .rpc
            .get_account_with_commitment(
                &self.config.bridge_state_pda,
                CommitmentConfig::confirmed(),
            )
            .await?
            .value
            .ok_or_else(|| BridgeError::AccountNotFound {
                address: self.config.bridge_state_pda.to_string(),
            })?;

        let bridge_state: &BridgeProgramStateWithDogeMint = bytemuck::from_bytes(&account.data);

        Ok(solana_sdk::pubkey::Pubkey::new_from_array(bridge_state.doge_mint))
    }

    /// Get the withdrawal snapshot from on-chain state.
    pub async fn snapshot_withdrawals_impl(&self) -> Result<PsyWithdrawalChainSnapshot, BridgeError> {
        let state = self.get_current_bridge_state_impl().await?;
        Ok(state.withdrawal_snapshot)
    }
}
