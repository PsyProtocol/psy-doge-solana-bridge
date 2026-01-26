//! Buffer management for Solana data accounts.
//!
//! This module provides parallel buffer building for:
//! - Pending mint buffers (groups of token mints)
//! - TXO buffers (transaction output indices)
//! - Generic buffers (arbitrary data)

pub mod manager;
pub mod pending_mint;
pub mod txo;

pub use manager::ParallelBufferManager;
pub use pending_mint::{derive_pending_mint_buffer_pda, PendingMintBufferBuilder};
pub use txo::{derive_txo_buffer_pda, TxoBufferBuilder};

/// Maximum chunk size for buffer writes (in bytes).
pub const CHUNK_SIZE: usize = 900;

/// Maximum data increase per resize operation (in bytes).
pub const MAX_DATA_INCREASE: usize = 10_240;
