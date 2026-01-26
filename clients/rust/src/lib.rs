//! Doge Bridge Client
//!
//! A Rust client for interacting with the Doge bridge on Solana.
//!
//! # Features
//!
//! - **Clean API**: Abstracted traits for all bridge operations
//! - **Rate Limiting**: Built-in token bucket rate limiting for RPC requests
//! - **Retry Logic**: Automatic retry with exponential backoff for transient failures
//! - **Parallel Buffer Building**: Efficient parallel construction of buffer accounts
//! - **Event Monitoring**: Stream bridge events in real-time
//! - **History Reconstruction**: Rebuild bridge state from on-chain data
//!
//! # Example
//!
//! ```ignore
//! use doge_bridge_client::{BridgeClient, BridgeApi};
//! use solana_sdk::pubkey::Pubkey;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = BridgeClient::new(
//!         "https://api.mainnet-beta.solana.com",
//!         &operator_keypair.to_bytes(),
//!         &payer_keypair.to_bytes(),
//!         bridge_state_pda,
//!         wormhole_core_program_id,
//!         wormhole_shim_program_id,
//!     )?;
//!
//!     // Get current bridge state
//!     let state = client.get_current_bridge_state().await?;
//!     println!("Block height: {}", state.bridge_header.finalized_state.block_height);
//!
//!     Ok(())
//! }
//! ```

// Core modules
pub mod api;
pub mod buffer;
pub mod client;
pub mod config;
pub mod constants;
pub mod errors;
pub mod history;
pub mod instructions;
pub mod monitor;
pub mod noop_shim_monitor;
pub mod rpc;
pub mod types;

// Legacy module (for backward compatibility)
#[allow(deprecated)]
pub mod bridge_client;
#[allow(deprecated)]
pub mod buffer_manager;

// Re-exports for convenient access
pub use api::{BridgeApi, ManualClaimApi, OperatorApi, WithdrawalApi};
pub use client::BridgeClient;
pub use config::{
    BridgeClientConfig, BridgeClientConfigBuilder, ParallelismConfig, RateLimitConfig, RetryConfig,
};
pub use errors::{BridgeError, BridgeResult, ErrorCategory};
pub use types::{
    CompactBridgeZKProof, DepositTxOutputRecord,
    FinalizedBlockMintTxoInfo, InitializeBridgeParams, PendingMint, ProcessMintsResult,
    PsyBridgeConfig, PsyBridgeHeader, PsyBridgeHeaderUpdate, PsyBridgeProgramState,
    PsyBridgeStateCommitment, PsyBridgeTipStateCommitment, PsyReturnTxOutput,
    PsyWithdrawalChainSnapshot, PsyWithdrawalRequest,
};

// Monitoring and history re-exports
pub use history::{
    BlockRecord, BridgeHistorySync, HistoryRecord, HistorySyncConfig, ManualDepositRecord,
    ProcessedWithdrawalRecord, SyncCheckpoint, SyncHandle, WithdrawalRequestRecord,
};
pub use monitor::{
    BridgeEvent, BridgeMonitor, ManualDepositClaimedEvent, MonitorConfig, MonitorHandle,
    WithdrawalProcessedEvent, WithdrawalRequestedEvent,
};
pub use noop_shim_monitor::{
    NoopShimMonitor, NoopShimMonitorConfig, NoopShimMonitorHandle, NoopShimWithdrawalMessage,
    WithdrawalPage, NOOP_SHIM_PROGRAM_ID,
};

// Backward compatibility re-exports
#[allow(deprecated)]
pub use errors::ClientError;
