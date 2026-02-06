//! Delegated manager set query implementations.
//!
//! Provides methods for querying the delegated manager set state from on-chain.

use async_trait::async_trait;
use psy_bridge_core::custodian_config::FullMultisigCustodianConfig;
use solana_sdk::commitment_config::CommitmentConfig;

use crate::{client::BridgeClient, errors::BridgeError};
use delegated_manager_set_types::{
    ManagerSet, ManagerSetIndex, DOGECOIN_CHAIN_ID, MANAGER_SET_DISC, MANAGER_SET_INDEX_DISC,
};

/// State of the delegated manager set.
#[derive(Clone, Debug)]
pub struct DelegatedManagerState {
    /// The current manager set index.
    pub current_index: u32,
    /// The chain ID (65 for Dogecoin).
    pub chain_id: u16,
    /// The manager set data containing the 7 compressed public keys.
    pub manager_set: ManagerSet,
}

impl DelegatedManagerState {
    /// Get the compressed public keys (231 bytes = 7 * 33).
    ///
    /// Returns the raw bytes of the 7 SEC1 compressed secp256k1 public keys.
    pub fn get_compressed_keys(&self) -> Result<&[u8], BridgeError> {
        self.manager_set
            .get_compressed_keys()
            .map_err(|_| BridgeError::InvalidBridgeState {
                message: "Invalid manager set data format".to_string(),
            })
    }

    /// Get individual compressed public keys as 33-byte slices.
    pub fn get_individual_keys(&self) -> Result<Vec<[u8; 33]>, BridgeError> {
        let keys_bytes = self.get_compressed_keys()?;
        let mut keys = Vec::with_capacity(7);
        for i in 0..7 {
            let start = i * 33;
            let end = start + 33;
            let mut key = [0u8; 33];
            key.copy_from_slice(&keys_bytes[start..end]);
            keys.push(key);
        }
        Ok(keys)
    }
}

/// API trait for querying delegated manager set state.
#[async_trait]
pub trait DelegatedManagerApi: Send + Sync {
    /// Get the current delegated manager set state for Dogecoin.
    ///
    /// Returns the current manager set including the 7 compressed public keys.
    async fn get_delegated_manager_state(&self) -> Result<DelegatedManagerState, BridgeError>;

    /// Get the current manager set index for Dogecoin.
    async fn get_manager_set_index(&self) -> Result<ManagerSetIndex, BridgeError>;

    /// Get a specific manager set by index.
    ///
    /// Useful for querying historical manager sets.
    async fn get_manager_set_at_index(&self, index: u32) -> Result<ManagerSet, BridgeError>;
}


/// API trait for querying delegated manager set state.
#[async_trait]
pub trait DelegatedManagerApiCore: Send + Sync {
    async fn get_current_delegated_manager_state_core(&self) -> anyhow::Result<FullMultisigCustodianConfig>;
}

impl BridgeClient {
    /// Get the current manager set index from on-chain.
    pub async fn get_manager_set_index_impl(&self) -> Result<ManagerSetIndex, BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let (index_pda, _) = ManagerSetIndex::pda(DOGECOIN_CHAIN_ID);

        self.retry_executor
            .execute(|| async {
                let account = self
                    .rpc
                    .get_account_with_commitment(&index_pda, CommitmentConfig::confirmed())
                    .await?
                    .value
                    .ok_or_else(|| BridgeError::AccountNotFound {
                        address: index_pda.to_string(),
                    })?;

                // Validate discriminator
                if account.data.len() < ManagerSetIndex::SIZE {
                    return Err(BridgeError::InvalidBridgeState {
                        message: "ManagerSetIndex account too small".to_string(),
                    });
                }

                if account.data[..8] != MANAGER_SET_INDEX_DISC {
                    return Err(BridgeError::InvalidBridgeState {
                        message: "Invalid ManagerSetIndex discriminator".to_string(),
                    });
                }

                let index: ManagerSetIndex =
                    borsh::BorshDeserialize::try_from_slice(&account.data[8..]).map_err(|e| {
                        BridgeError::InvalidBridgeState {
                            message: format!("Failed to deserialize ManagerSetIndex: {}", e),
                        }
                    })?;

                Ok(index)
            })
            .await
    }

    /// Get a manager set at a specific index from on-chain.
    pub async fn get_manager_set_at_index_impl(
        &self,
        index: u32,
    ) -> Result<ManagerSet, BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let (set_pda, _) = ManagerSet::pda(DOGECOIN_CHAIN_ID, index);

        self.retry_executor
            .execute(|| async {
                let account = self
                    .rpc
                    .get_account_with_commitment(&set_pda, CommitmentConfig::confirmed())
                    .await?
                    .value
                    .ok_or_else(|| BridgeError::AccountNotFound {
                        address: set_pda.to_string(),
                    })?;

                // Validate discriminator
                if account.data.len() < 8 {
                    return Err(BridgeError::InvalidBridgeState {
                        message: "ManagerSet account too small".to_string(),
                    });
                }

                if account.data[..8] != MANAGER_SET_DISC {
                    return Err(BridgeError::InvalidBridgeState {
                        message: "Invalid ManagerSet discriminator".to_string(),
                    });
                }

                let set: ManagerSet =
                    borsh::BorshDeserialize::try_from_slice(&account.data[8..]).map_err(|e| {
                        BridgeError::InvalidBridgeState {
                            message: format!("Failed to deserialize ManagerSet: {}", e),
                        }
                    })?;

                Ok(set)
            })
            .await
    }

    /// Get the current delegated manager state.
    pub async fn get_delegated_manager_state_impl(
        &self,
    ) -> Result<DelegatedManagerState, BridgeError> {
        // First get the current index
        let index = self.get_manager_set_index_impl().await?;

        // Then get the manager set at that index
        let manager_set = self.get_manager_set_at_index_impl(index.current_index).await?;

        Ok(DelegatedManagerState {
            current_index: index.current_index,
            chain_id: index.manager_chain_id,
            manager_set,
        })
    }
}

#[async_trait]
impl DelegatedManagerApi for BridgeClient {
    async fn get_delegated_manager_state(&self) -> Result<DelegatedManagerState, BridgeError> {
        self.get_delegated_manager_state_impl().await
    }

    async fn get_manager_set_index(&self) -> Result<ManagerSetIndex, BridgeError> {
        self.get_manager_set_index_impl().await
    }

    async fn get_manager_set_at_index(&self, index: u32) -> Result<ManagerSet, BridgeError> {
        self.get_manager_set_at_index_impl(index).await
    }
}
#[async_trait]
impl DelegatedManagerApiCore for BridgeClient {
    async fn get_current_delegated_manager_state_core(&self) -> anyhow::Result<FullMultisigCustodianConfig> {
        let state = self.get_delegated_manager_state().await?;

        let custodian_public_keys = state.get_individual_keys()?;

        let custodian_config_id = 0; // Assuming a fixed config ID for delegated manager
        let network_id = 1; // Assuming mainnet

        let bridge_pda = self.bridge_state_pda().to_bytes();

        let keys: [[u8; 33]; 7] = custodian_public_keys
            .try_into()
            .map_err(|_| anyhow::anyhow!("Expected exactly 7 compressed public keys"))?;
        FullMultisigCustodianConfig::from_compressed_public_keys(
            bridge_pda,
            keys,
            custodian_config_id,
            network_id,
        )
    }
}
