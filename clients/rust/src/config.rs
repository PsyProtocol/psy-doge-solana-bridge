//! Configuration types for the BridgeClient.
//!
//! This module provides configuration structs for rate limiting, retries,
//! parallelism, and the main client configuration.

use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use std::sync::Arc;

use crate::constants::{
    DOGE_BRIDGE_PROGRAM_ID, GENERIC_BUFFER_BUILDER_PROGRAM_ID, MANUAL_CLAIM_PROGRAM_ID,
    PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID, TXO_BUFFER_BUILDER_PROGRAM_ID,
};

/// Rate limiting configuration for RPC requests.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per second
    pub max_rps: u32,
    /// Burst capacity for token bucket
    pub burst_size: u32,
    /// Whether to queue requests when rate limited
    pub queue_on_limit: bool,
    /// Maximum queue depth before rejecting
    pub max_queue_depth: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_rps: 10,
            burst_size: 20,
            queue_on_limit: true,
            max_queue_depth: 100,
        }
    }
}

/// Retry configuration for failed operations.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial delay between retries in milliseconds
    pub initial_delay_ms: u64,
    /// Maximum delay between retries in milliseconds
    pub max_delay_ms: u64,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Transaction confirmation timeout in milliseconds
    pub confirmation_timeout_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 10,
            initial_delay_ms: 500,
            max_delay_ms: 30_000,
            backoff_multiplier: 2.0,
            confirmation_timeout_ms: 60_000,
        }
    }
}

/// Parallelism configuration for buffer operations.
#[derive(Debug, Clone)]
pub struct ParallelismConfig {
    /// Maximum concurrent write operations
    pub max_concurrent_writes: usize,
    /// Maximum concurrent resize operations
    pub max_concurrent_resizes: usize,
    /// Batch size for group insertions
    pub group_batch_size: usize,
}

impl Default for ParallelismConfig {
    fn default() -> Self {
        Self {
            max_concurrent_writes: 4,
            max_concurrent_resizes: 2,
            group_batch_size: 4,
        }
    }
}

/// Main configuration for the BridgeClient.
#[derive(Clone)]
pub struct BridgeClientConfig {
    /// Solana RPC URL
    pub rpc_url: String,
    /// Bridge state PDA
    pub bridge_state_pda: Pubkey,
    /// Operator keypair (signs operator-only transactions)
    pub operator: Arc<Keypair>,
    /// Payer keypair (pays for transaction fees)
    pub payer: Arc<Keypair>,
    /// DOGE mint address (can be fetched from state if None)
    pub doge_mint: Option<Pubkey>,
    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,
    /// Retry configuration
    pub retry: RetryConfig,
    /// Parallelism configuration
    pub parallelism: ParallelismConfig,
    /// Doge bridge program ID
    pub program_id: Pubkey,
    /// Manual claim program ID
    pub manual_claim_program_id: Pubkey,
    /// Pending mint buffer program ID
    pub pending_mint_program_id: Pubkey,
    /// TXO buffer program ID
    pub txo_buffer_program_id: Pubkey,
    /// Generic buffer program ID
    pub generic_buffer_program_id: Pubkey,
    /// Wormhole core program ID
    pub wormhole_core_program_id: Pubkey,
    /// Wormhole shim program ID
    pub wormhole_shim_program_id: Pubkey,
}

impl std::fmt::Debug for BridgeClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BridgeClientConfig")
            .field("rpc_url", &self.rpc_url)
            .field("bridge_state_pda", &self.bridge_state_pda)
            .field("operator", &self.operator.pubkey())
            .field("payer", &self.payer.pubkey())
            .field("doge_mint", &self.doge_mint)
            .field("rate_limit", &self.rate_limit)
            .field("retry", &self.retry)
            .field("parallelism", &self.parallelism)
            .finish()
    }
}

/// Builder for BridgeClientConfig.
#[derive(Default)]
pub struct BridgeClientConfigBuilder {
    rpc_url: Option<String>,
    bridge_state_pda: Option<Pubkey>,
    operator: Option<Arc<Keypair>>,
    payer: Option<Arc<Keypair>>,
    doge_mint: Option<Pubkey>,
    rate_limit: Option<RateLimitConfig>,
    retry: Option<RetryConfig>,
    parallelism: Option<ParallelismConfig>,
    program_id: Option<Pubkey>,
    manual_claim_program_id: Option<Pubkey>,
    pending_mint_program_id: Option<Pubkey>,
    txo_buffer_program_id: Option<Pubkey>,
    generic_buffer_program_id: Option<Pubkey>,
    wormhole_core_program_id: Option<Pubkey>,
    wormhole_shim_program_id: Option<Pubkey>,
}

impl BridgeClientConfigBuilder {
    /// Create a new builder with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the RPC URL.
    pub fn rpc_url(mut self, url: impl Into<String>) -> Self {
        self.rpc_url = Some(url.into());
        self
    }

    /// Set the bridge state PDA.
    pub fn bridge_state_pda(mut self, pda: Pubkey) -> Self {
        self.bridge_state_pda = Some(pda);
        self
    }

    /// Set the operator keypair.
    pub fn operator(mut self, keypair: Keypair) -> Self {
        self.operator = Some(Arc::new(keypair));
        self
    }

    /// Set the operator keypair from an Arc.
    pub fn operator_arc(mut self, keypair: Arc<Keypair>) -> Self {
        self.operator = Some(keypair);
        self
    }

    /// Set the payer keypair.
    pub fn payer(mut self, keypair: Keypair) -> Self {
        self.payer = Some(Arc::new(keypair));
        self
    }

    /// Set the payer keypair from an Arc.
    pub fn payer_arc(mut self, keypair: Arc<Keypair>) -> Self {
        self.payer = Some(keypair);
        self
    }

    /// Set both operator and payer to the same keypair.
    pub fn operator_and_payer(mut self, keypair: Keypair) -> Self {
        let kp = Arc::new(keypair);
        self.operator = Some(kp.clone());
        self.payer = Some(kp);
        self
    }

    /// Set the DOGE mint address.
    pub fn doge_mint(mut self, mint: Pubkey) -> Self {
        self.doge_mint = Some(mint);
        self
    }

    /// Set the rate limiting configuration.
    pub fn rate_limit(mut self, config: RateLimitConfig) -> Self {
        self.rate_limit = Some(config);
        self
    }

    /// Set the retry configuration.
    pub fn retry(mut self, config: RetryConfig) -> Self {
        self.retry = Some(config);
        self
    }

    /// Set the parallelism configuration.
    pub fn parallelism(mut self, config: ParallelismConfig) -> Self {
        self.parallelism = Some(config);
        self
    }

    /// Set the doge bridge program ID.
    pub fn program_id(mut self, id: Pubkey) -> Self {
        self.program_id = Some(id);
        self
    }

    /// Set the manual claim program ID.
    pub fn manual_claim_program_id(mut self, id: Pubkey) -> Self {
        self.manual_claim_program_id = Some(id);
        self
    }

    /// Set the pending mint buffer program ID.
    pub fn pending_mint_program_id(mut self, id: Pubkey) -> Self {
        self.pending_mint_program_id = Some(id);
        self
    }

    /// Set the TXO buffer program ID.
    pub fn txo_buffer_program_id(mut self, id: Pubkey) -> Self {
        self.txo_buffer_program_id = Some(id);
        self
    }

    /// Set the generic buffer program ID.
    pub fn generic_buffer_program_id(mut self, id: Pubkey) -> Self {
        self.generic_buffer_program_id = Some(id);
        self
    }

    /// Set the Wormhole core program ID.
    pub fn wormhole_core_program_id(mut self, id: Pubkey) -> Self {
        self.wormhole_core_program_id = Some(id);
        self
    }

    /// Set the Wormhole shim program ID.
    pub fn wormhole_shim_program_id(mut self, id: Pubkey) -> Self {
        self.wormhole_shim_program_id = Some(id);
        self
    }

    /// Build the configuration.
    ///
    /// Returns an error if required fields are missing.
    pub fn build(self) -> Result<BridgeClientConfig, ConfigError> {
        let rpc_url = self.rpc_url.ok_or(ConfigError::MissingField("rpc_url"))?;
        let bridge_state_pda = self
            .bridge_state_pda
            .ok_or(ConfigError::MissingField("bridge_state_pda"))?;
        let operator = self.operator.ok_or(ConfigError::MissingField("operator"))?;
        let payer = self.payer.ok_or(ConfigError::MissingField("payer"))?;

        // Wormhole program IDs are required for withdrawal processing
        let wormhole_core_program_id = self
            .wormhole_core_program_id
            .ok_or(ConfigError::MissingField("wormhole_core_program_id"))?;
        let wormhole_shim_program_id = self
            .wormhole_shim_program_id
            .ok_or(ConfigError::MissingField("wormhole_shim_program_id"))?;

        Ok(BridgeClientConfig {
            rpc_url,
            bridge_state_pda,
            operator,
            payer,
            doge_mint: self.doge_mint,
            rate_limit: self.rate_limit.unwrap_or_default(),
            retry: self.retry.unwrap_or_default(),
            parallelism: self.parallelism.unwrap_or_default(),
            program_id: self.program_id.unwrap_or(DOGE_BRIDGE_PROGRAM_ID),
            manual_claim_program_id: self
                .manual_claim_program_id
                .unwrap_or(MANUAL_CLAIM_PROGRAM_ID),
            pending_mint_program_id: self
                .pending_mint_program_id
                .unwrap_or(PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID),
            txo_buffer_program_id: self
                .txo_buffer_program_id
                .unwrap_or(TXO_BUFFER_BUILDER_PROGRAM_ID),
            generic_buffer_program_id: self
                .generic_buffer_program_id
                .unwrap_or(GENERIC_BUFFER_BUILDER_PROGRAM_ID),
            wormhole_core_program_id,
            wormhole_shim_program_id,
        })
    }
}

/// Error type for configuration issues.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing required field: {0}")]
    MissingField(&'static str),

    #[error("Invalid configuration: {0}")]
    Invalid(String),

    #[error("Invalid keypair: {0}")]
    InvalidKeypair(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_configs() {
        let rate_limit = RateLimitConfig::default();
        assert_eq!(rate_limit.max_rps, 10);
        assert_eq!(rate_limit.burst_size, 20);

        let retry = RetryConfig::default();
        assert_eq!(retry.max_retries, 10);
        assert_eq!(retry.initial_delay_ms, 500);

        let parallelism = ParallelismConfig::default();
        assert_eq!(parallelism.max_concurrent_writes, 4);
    }

    #[test]
    fn test_builder_missing_fields() {
        let result = BridgeClientConfigBuilder::new().build();
        assert!(result.is_err());
    }
}
