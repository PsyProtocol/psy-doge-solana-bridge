//! Unit tests for BridgeClient components.
//!
//! These tests focus on the internal logic of:
//! - Rate limiting
//! - Retry behavior
//! - Buffer builders
//! - Configuration

use std::time::{Duration, Instant};

use doge_bridge_client::{
    buffer::{
        derive_pending_mint_buffer_pda, derive_txo_buffer_pda, PendingMintBufferBuilder,
        TxoBufferBuilder, CHUNK_SIZE,
    },
    config::{
        BridgeClientConfigBuilder, ParallelismConfig, RateLimitConfig, RetryConfig,
    },
    errors::{BridgeError, ErrorCategory},
    rpc::{RpcRateLimiter, RetryExecutor},
};
use psy_doge_solana_core::data_accounts::pending_mint::PendingMint;
use solana_sdk::{pubkey::Pubkey, signature::Keypair};

// ============================================================================
// Configuration Tests
// ============================================================================

#[test]
fn test_default_config() {
    let rate_limit = RateLimitConfig::default();
    assert_eq!(rate_limit.max_rps, 10);
    assert_eq!(rate_limit.burst_size, 20);

    let retry = RetryConfig::default();
    assert_eq!(retry.max_retries, 10);
    assert!(retry.initial_delay_ms > 0);

    let parallelism = ParallelismConfig::default();
    assert!(parallelism.max_concurrent_writes > 0);
}

#[test]
fn test_config_builder_basic() {
        let payer = Keypair::new();
        let operator = Keypair::from_bytes(&payer.to_bytes()).unwrap();
    let bridge_state_pda = Pubkey::new_unique();
    let wormhole_core = Pubkey::new_unique();
    let wormhole_shim = Pubkey::new_unique();

    let config = BridgeClientConfigBuilder::new()
        .rpc_url("http://localhost:8899")
        .bridge_state_pda(bridge_state_pda)
        .operator(operator)
        .payer(payer)
        .wormhole_core_program_id(wormhole_core)
        .wormhole_shim_program_id(wormhole_shim)
        .rate_limit(RateLimitConfig {
            max_rps: 50,
            burst_size: 100,
            queue_on_limit: true,
            max_queue_depth: 500,
        })
        .retry(RetryConfig {
            max_retries: 5,
            initial_delay_ms: 200,
            max_delay_ms: 5000,
            backoff_multiplier: 2.0,
            confirmation_timeout_ms: 120_000,
        })
        .parallelism(ParallelismConfig {
            max_concurrent_writes: 10,
            max_concurrent_resizes: 3,
            group_batch_size: 5,
        })
        .doge_mint(Pubkey::new_unique())
        .build()
        .expect("Failed to build config");

    assert_eq!(config.rate_limit.max_rps, 50);
    assert_eq!(config.retry.max_retries, 5);
    assert_eq!(config.parallelism.max_concurrent_writes, 10);
    assert!(config.doge_mint.is_some());
}

#[test]
fn test_config_builder_missing_required_fields() {
    // Missing all required fields
    let result = BridgeClientConfigBuilder::new().build();
    assert!(result.is_err());

    // Missing wormhole program IDs
    let payer = Keypair::new();
        let operator = Keypair::from_bytes(&payer.to_bytes()).unwrap();
    let result = BridgeClientConfigBuilder::new()
        .rpc_url("http://localhost:8899")
        .bridge_state_pda(Pubkey::new_unique())
        .operator(operator)
        .payer(payer)
        .build();
    assert!(result.is_err());
}

#[test]
fn test_config_custom_program_ids() {
    let custom_program = Pubkey::new_unique();

    let config = BridgeClientConfigBuilder::new()
        .rpc_url("http://localhost:8899")
        .bridge_state_pda(Pubkey::new_unique())
        .operator(Keypair::new())
        .payer(Keypair::new())
        .wormhole_core_program_id(Pubkey::new_unique())
        .wormhole_shim_program_id(Pubkey::new_unique())
        .program_id(custom_program)
        .build()
        .expect("Failed to build config");

    assert_eq!(config.program_id, custom_program);
}

#[test]
fn test_config_operator_and_payer_same() {
    let keypair = Keypair::new();
    let keypair_pubkey = keypair.pubkey();

    let config = BridgeClientConfigBuilder::new()
        .rpc_url("http://localhost:8899")
        .bridge_state_pda(Pubkey::new_unique())
        .operator_and_payer(keypair)
        .wormhole_core_program_id(Pubkey::new_unique())
        .wormhole_shim_program_id(Pubkey::new_unique())
        .build()
        .expect("Failed to build config");

    use solana_sdk::signer::Signer;
    assert_eq!(config.operator.pubkey(), keypair_pubkey);
    assert_eq!(config.payer.pubkey(), keypair_pubkey);
}

// ============================================================================
// Error Tests
// ============================================================================

#[test]
fn test_error_is_retryable() {
    // Retryable errors
    assert!(BridgeError::ConnectionTimeout.is_retryable());
    assert!(BridgeError::RateLimited { retry_after_ms: 100 }.is_retryable());

    // Non-retryable errors
    assert!(!BridgeError::InvalidInput("bad input".to_string()).is_retryable());
    assert!(!BridgeError::SignerError.is_retryable());
    assert!(!BridgeError::AccountNotFound {
        address: "test".to_string()
    }
    .is_retryable());
}

#[test]
fn test_error_retry_hint() {
    let rate_limited = BridgeError::RateLimited { retry_after_ms: 500 };
    assert_eq!(rate_limited.retry_hint_ms(), Some(500));

    let timeout = BridgeError::ConnectionTimeout;
    assert!(timeout.retry_hint_ms().is_some());

    let invalid = BridgeError::InvalidInput("test".to_string());
    assert!(invalid.retry_hint_ms().is_none());
}

#[test]
fn test_error_category() {
    assert_eq!(
        BridgeError::ConnectionTimeout.category(),
        ErrorCategory::Network
    );
    assert_eq!(
        BridgeError::TransactionRejected {
            reason: "test".to_string()
        }
        .category(),
        ErrorCategory::Transaction
    );
    assert_eq!(
        BridgeError::InvalidInput("test".to_string()).category(),
        ErrorCategory::Validation
    );
    assert_eq!(
        BridgeError::BufferCreationFailed {
            message: "test".to_string()
        }
        .category(),
        ErrorCategory::Buffer
    );
}

// ============================================================================
// Rate Limiter Tests
// ============================================================================

#[tokio::test]
async fn test_rate_limiter_basic() {
    let config = RateLimitConfig {
        max_rps: 10,
        burst_size: 10,
        queue_on_limit: true,
        max_queue_depth: 100,
    };
    let limiter = RpcRateLimiter::new(config);

    // Should be able to acquire immediately (within burst)
    let start = Instant::now();
    for _ in 0..5 {
        let _guard = limiter.acquire().await.unwrap();
    }
    let elapsed = start.elapsed();

    // Should complete quickly since we're within burst
    assert!(elapsed < Duration::from_millis(500));
}

// ============================================================================
// Retry Executor Tests
// ============================================================================

#[tokio::test]
async fn test_retry_executor_success_first_try() {
    let config = RetryConfig::default();
    let executor = RetryExecutor::new(config);

    let result: Result<u32, BridgeError> = executor.execute(|| async { Ok(42) }).await;

    assert_eq!(result.unwrap(), 42);
}

#[tokio::test]
async fn test_retry_executor_success_after_retries() {
    let config = RetryConfig {
        max_retries: 3,
        initial_delay_ms: 10,
        max_delay_ms: 100,
        backoff_multiplier: 2.0,
        confirmation_timeout_ms: 1000,
    };
    let executor = RetryExecutor::new(config);

    let attempts = std::sync::atomic::AtomicU32::new(0);

    let result: Result<u32, BridgeError> = executor
        .execute(|| {
            let attempt = attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async move {
                if attempt < 2 {
                    Err(BridgeError::ConnectionTimeout)
                } else {
                    Ok(42)
                }
            }
        })
        .await;

    assert_eq!(result.unwrap(), 42);
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_retry_executor_non_retryable_error() {
    let config = RetryConfig {
        max_retries: 5,
        initial_delay_ms: 10,
        max_delay_ms: 100,
        backoff_multiplier: 2.0,
        confirmation_timeout_ms: 1000,
    };
    let executor = RetryExecutor::new(config);

    let attempts = std::sync::atomic::AtomicU32::new(0);

    let result: Result<u32, BridgeError> = executor
        .execute(|| {
            attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async { Err(BridgeError::InvalidInput("test".to_string())) }
        })
        .await;

    assert!(result.is_err());
    // Should only try once since error is not retryable
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_retry_executor_max_retries_exceeded() {
    let config = RetryConfig {
        max_retries: 2,
        initial_delay_ms: 10,
        max_delay_ms: 100,
        backoff_multiplier: 2.0,
        confirmation_timeout_ms: 1000,
    };
    let executor = RetryExecutor::new(config);

    let attempts = std::sync::atomic::AtomicU32::new(0);

    let result: Result<u32, BridgeError> = executor
        .execute(|| {
            attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            async { Err(BridgeError::ConnectionTimeout) }
        })
        .await;

    assert!(result.is_err());
    // Initial + 2 retries = 3 attempts
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
}

// ============================================================================
// Pending Mint Buffer Builder Tests
// ============================================================================

fn create_test_mint(idx: u8) -> PendingMint {
    PendingMint {
        recipient: [idx; 32],
        amount: idx as u64 * 1000,
    }
}

#[test]
fn test_pending_mint_builder_empty() {
    let builder = PendingMintBufferBuilder::new(vec![]);
    assert_eq!(builder.total_mints(), 0);
    assert_eq!(builder.num_groups(), 0);
}

#[test]
fn test_pending_mint_builder_single_group() {
    // Add 24 mints (exactly 1 group)
    let mints: Vec<PendingMint> = (0..24).map(|i| create_test_mint(i as u8)).collect();
    let builder = PendingMintBufferBuilder::new(mints);

    assert_eq!(builder.total_mints(), 24);
    assert_eq!(builder.num_groups(), 1);

    let group = builder.get_group(0);
    assert_eq!(group.len(), 24);
}

#[test]
fn test_pending_mint_builder_multiple_groups() {
    // Add 50 mints (3 groups: 24 + 24 + 2)
    let mints: Vec<PendingMint> = (0..50).map(|i| create_test_mint(i as u8)).collect();
    let builder = PendingMintBufferBuilder::new(mints);

    assert_eq!(builder.total_mints(), 50);
    assert_eq!(builder.num_groups(), 3);

    assert_eq!(builder.get_group(0).len(), 24);
    assert_eq!(builder.get_group(1).len(), 24);
    assert_eq!(builder.get_group(2).len(), 2);
}

#[test]
fn test_pending_mint_builder_edge_cases() {
    // Exactly at group boundary (48 = 2 * 24)
    let mints: Vec<PendingMint> = (0..48).map(|i| create_test_mint(i as u8)).collect();
    let builder = PendingMintBufferBuilder::new(mints);
    assert_eq!(builder.num_groups(), 2);

    // One over group boundary
    let mints: Vec<PendingMint> = (0..25).map(|i| create_test_mint(i as u8)).collect();
    let builder = PendingMintBufferBuilder::new(mints);
    assert_eq!(builder.num_groups(), 2);
}

#[test]
fn test_pending_mint_serialize_group() {
    let mints: Vec<PendingMint> = (0..5).map(|i| create_test_mint(i as u8)).collect();
    let builder = PendingMintBufferBuilder::new(mints);

    let data = builder.serialize_group(0);
    // PendingMint is 40 bytes (32 recipient + 8 amount)
    assert_eq!(data.len(), 5 * 40);
}

// ============================================================================
// TXO Buffer Builder Tests
// ============================================================================

#[test]
fn test_txo_builder_empty() {
    let builder = TxoBufferBuilder::new(vec![], 100);
    assert_eq!(builder.total_indices(), 0);
    assert_eq!(builder.data_size(), 0);
    assert_eq!(builder.num_chunks(), 0);
    assert_eq!(builder.block_height(), 100);
}

#[test]
fn test_txo_builder_small() {
    let indices: Vec<u32> = (0..10).collect();
    let builder = TxoBufferBuilder::new(indices, 100);

    assert_eq!(builder.total_indices(), 10);
    assert_eq!(builder.data_size(), 40); // 10 * 4 bytes
    assert_eq!(builder.num_chunks(), 1);
}

#[test]
fn test_txo_builder_multiple_chunks() {
    // CHUNK_SIZE = 900, each u32 = 4 bytes
    // 900 / 4 = 225 indices per chunk
    let indices: Vec<u32> = (0..500).collect();
    let builder = TxoBufferBuilder::new(indices, 100);

    assert_eq!(builder.total_indices(), 500);
    assert_eq!(builder.data_size(), 2000);
    // 2000 / 900 = 2.22, so 3 chunks
    assert_eq!(builder.num_chunks(), 3);
}

#[test]
fn test_txo_builder_chunks() {
    let indices: Vec<u32> = (0..300).collect();
    let builder = TxoBufferBuilder::new(indices, 100);

    let chunks = builder.chunks();

    // 300 indices * 4 bytes = 1200 bytes
    // 1200 / 900 = 2 chunks (ceil)
    assert_eq!(chunks.len(), 2);

    // First chunk should start at offset 0
    assert_eq!(chunks[0].0, 0);

    // Second chunk should start at CHUNK_SIZE
    assert_eq!(chunks[1].0, CHUNK_SIZE);
}

#[test]
fn test_txo_builder_serialize_all() {
    let indices = vec![1u32, 2, 3];
    let builder = TxoBufferBuilder::new(indices, 100);

    let data = builder.serialize_all();
    assert_eq!(data.len(), 12);
    assert_eq!(&data[0..4], &1u32.to_le_bytes());
    assert_eq!(&data[4..8], &2u32.to_le_bytes());
    assert_eq!(&data[8..12], &3u32.to_le_bytes());
}

// ============================================================================
// PDA Derivation Tests
// ============================================================================

#[test]
fn test_pending_mint_buffer_pda_derivation() {
    let program_id = Pubkey::new_unique();
    let payer = Pubkey::new_unique();

    let (pda, bump) = derive_pending_mint_buffer_pda(&program_id, &payer);

    // Should be a valid PDA
    assert!(pda != Pubkey::default());

    // Should be deterministic
    let (pda2, bump2) = derive_pending_mint_buffer_pda(&program_id, &payer);
    assert_eq!(pda, pda2);
    assert_eq!(bump, bump2);

    // Different inputs should give different PDAs
    let different_payer = Pubkey::new_unique();
    let (different_pda, _) = derive_pending_mint_buffer_pda(&program_id, &different_payer);
    assert_ne!(pda, different_pda);
}

#[test]
fn test_txo_buffer_pda_derivation() {
    let program_id = Pubkey::new_unique();
    let payer = Pubkey::new_unique();

    let (pda, bump) = derive_txo_buffer_pda(&program_id, &payer);

    assert!(pda != Pubkey::default());

    // Should be deterministic
    let (pda2, bump2) = derive_txo_buffer_pda(&program_id, &payer);
    assert_eq!(pda, pda2);
    assert_eq!(bump, bump2);
}

// ============================================================================
// Integration-style Unit Tests
// ============================================================================

#[test]
fn test_pending_mint_serialization_roundtrip() {
    let mint = PendingMint {
        recipient: [42u8; 32],
        amount: 100_000_000,
    };

    let builder = PendingMintBufferBuilder::new(vec![mint]);
    let groups: Vec<_> = builder.groups().collect();

    assert_eq!(groups.len(), 1);
    let (idx, group_mints) = groups[0];
    assert_eq!(idx, 0);
    assert_eq!(group_mints.len(), 1);

    let mint1 = &group_mints[0];
    assert_eq!(mint1.recipient, [42u8; 32]);
    assert_eq!(mint1.amount, 100_000_000);
}

#[test]
fn test_large_mint_batch() {
    // Test with 100 mints (5 groups: 24*4 + 4 = 100)
    let mints: Vec<PendingMint> = (0..100)
        .map(|i| PendingMint {
            recipient: [i as u8; 32],
            amount: i as u64 * 1_000_000,
        })
        .collect();

    let builder = PendingMintBufferBuilder::new(mints);
    assert_eq!(builder.total_mints(), 100);
    assert_eq!(builder.num_groups(), 5); // 100 / 24 = 4.16, ceil = 5

    // Verify each group
    let groups: Vec<_> = builder.groups().collect();
    assert_eq!(groups.len(), 5);
    assert_eq!(groups[0].1.len(), 24);
    assert_eq!(groups[1].1.len(), 24);
    assert_eq!(groups[2].1.len(), 24);
    assert_eq!(groups[3].1.len(), 24);
    assert_eq!(groups[4].1.len(), 4);
}

#[test]
fn test_large_txo_batch() {
    // Test with 1000 TXO indices
    let indices: Vec<u32> = (0..1000).collect();
    let builder = TxoBufferBuilder::new(indices, 12345);

    assert_eq!(builder.total_indices(), 1000);
    assert_eq!(builder.data_size(), 4000);
    assert_eq!(builder.block_height(), 12345);

    // 4000 / 900 = 4.44, so 5 chunks
    assert_eq!(builder.num_chunks(), 5);

    let chunks = builder.chunks();
    assert_eq!(chunks.len(), 5);

    // Verify chunk offsets
    for (i, (offset, _)) in chunks.iter().enumerate() {
        assert_eq!(*offset, i * CHUNK_SIZE);
    }
}
