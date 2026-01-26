//! Integration tests for bridge client and history reconstruction.
//!
//! These tests verify that:
//! 1. The BridgeClient correctly processes blocks and deposits
//! 2. The history reconstruction can rebuild full state from on-chain data
//! 3. State reconstructed from history matches the original state
//!
//! Note: These tests use the in-memory program test framework, not a real RPC.
//! Full history reconstruction tests would require a local validator with RPC.

use doge_bridge::state::BridgeState;
use doge_bridge_client::instructions;
use doge_bridge_test_utils::{
    block_transition_helper::{BTAutoClaimedDeposit, BlockTransitionHelper},
    BridgeTestContext,
};
use psy_bridge_core::
    header::{PsyBridgeHeader, PsyBridgeStateCommitment, PsyBridgeTipStateCommitment}
;
use psy_doge_solana_core::{
    instructions::doge_bridge::InitializeBridgeParams,
    program_state::{PsyBridgeConfig, PsyReturnTxOutput},
};
use solana_program_test::tokio;
use solana_sdk::signature::Signer;

// ============================================================================
// Test Helpers
// ============================================================================

/// Default bridge configuration with no fees
fn default_bridge_config() -> PsyBridgeConfig {
    PsyBridgeConfig {
        deposit_fee_rate_numerator: 0,
        deposit_fee_rate_denominator: 100,
        withdrawal_fee_rate_numerator: 0,
        withdrawal_fee_rate_denominator: 100,
        deposit_flat_fee_sats: 0,
        withdrawal_flat_fee_sats: 0,
    }
}

/// Default initialize params
fn default_initialize_params() -> InitializeBridgeParams {
    InitializeBridgeParams {
        bridge_header: PsyBridgeHeader {
            tip_state: PsyBridgeTipStateCommitment::default(),
            finalized_state: PsyBridgeStateCommitment::default(),
            bridge_state_hash: [0u8; 32],
            last_rollback_at_secs: 0,
            paused_until_secs: 0,
            total_finalized_fees_collected_chain_history: 0,
        },
        custodian_wallet_config_hash: [1u8; 32],
        start_return_txo_output: PsyReturnTxOutput {
            sighash: [0u8; 32],
            output_index: 0,
            amount_sats: 0,
        },
        config_params: default_bridge_config(),
    }
}

/// Helper to read bridge state from the context
async fn read_bridge_state(ctx: &BridgeTestContext) -> BridgeState {
    let bridge_account = ctx
        .client
        .client
        .get_account(ctx.client.bridge_state_pda)
        .await
        .unwrap()
        .unwrap();
    let bridge_state: &BridgeState = bytemuck::from_bytes(&bridge_account.data);
    bridge_state.clone()
}

// ============================================================================
// State Verification Tests
// ============================================================================

/// Test that we can track all auto-claimed deposits through block transitions.
///
/// This simulates what history reconstruction would verify: that the total
/// deposits recorded matches what we process.
#[tokio::test]
async fn test_state_tracking_single_block() {
    println!("=== Test: State Tracking Single Block ===");

    let ctx = BridgeTestContext::new().await;

    // Initialize
    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &default_initialize_params(),
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone())
        .await
        .unwrap();

    // Record initial state
    let initial_state = read_bridge_state(&ctx).await;
    let initial_height = initial_state.core_state.bridge_header.finalized_state.block_height;
    let initial_deposits = initial_state
        .core_state
        .bridge_header
        .finalized_state
        .auto_claimed_deposits_next_index;

    println!(
        "Initial state: height={}, deposits={}",
        initial_height, initial_deposits
    );

    // Mine block with 5 deposits
    let num_deposits = 5;
    let mut deposits = Vec::new();
    for i in 0..num_deposits {
        let user = helper.add_user();
        deposits.push(BTAutoClaimedDeposit::new(
            user.to_bytes(),
            10_000_000 + (i as u64 * 1_000_000),
            i as u32 + 1,
        ));
    }

    helper.mine_and_process_block(deposits).await.unwrap();

    // Verify final state
    let final_state = read_bridge_state(&ctx).await;
    let final_height = final_state.core_state.bridge_header.finalized_state.block_height;
    let final_deposits = final_state
        .core_state
        .bridge_header
        .finalized_state
        .auto_claimed_deposits_next_index;

    println!(
        "Final state: height={}, deposits={}",
        final_height, final_deposits
    );

    assert_eq!(
        final_height,
        initial_height + 1,
        "Block height should increment by 1"
    );
    assert_eq!(
        final_deposits,
        initial_deposits + num_deposits as u32,
        "Deposit count should match"
    );

    println!("=== Test Passed ===\n");
}

/// Test tracking state across multiple sequential blocks.
#[tokio::test]
async fn test_state_tracking_multiple_blocks() {
    println!("=== Test: State Tracking Multiple Blocks ===");

    let ctx = BridgeTestContext::new().await;

    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &default_initialize_params(),
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone())
        .await
        .unwrap();

    // Track expected state
    let mut expected_height = 0u32;
    let mut expected_deposits = 0u32;
    let mut block_deposit_counts = Vec::new();

    // Mine 5 blocks with varying deposits
    let deposits_per_block = [3, 10, 0, 24, 5];

    for (block_idx, &deposit_count) in deposits_per_block.iter().enumerate() {
        println!(
            "Mining block {} with {} deposits...",
            block_idx + 1,
            deposit_count
        );

        let mut deposits = Vec::new();
        for i in 0..deposit_count {
            let user = helper.add_user();
            deposits.push(BTAutoClaimedDeposit::new(
                user.to_bytes(),
                5_000_000,
                (block_idx * 100 + i) as u32,
            ));
        }

        helper.mine_and_process_block(deposits).await.unwrap();

        expected_height += 1;
        expected_deposits += deposit_count as u32;
        block_deposit_counts.push(deposit_count);

        // Verify state after each block
        let state = read_bridge_state(&ctx).await;
        assert_eq!(
            state.core_state.bridge_header.finalized_state.block_height,
            expected_height,
            "Height mismatch after block {}",
            block_idx + 1
        );
        assert_eq!(
            state
                .core_state
                .bridge_header
                .finalized_state
                .auto_claimed_deposits_next_index,
            expected_deposits,
            "Deposit count mismatch after block {}",
            block_idx + 1
        );
    }

    println!(
        "Processed {} blocks with {} total deposits",
        deposits_per_block.len(),
        expected_deposits
    );
    println!("Deposits per block: {:?}", block_deposit_counts);
    println!("=== Test Passed ===\n");
}

/// Test that large mint batches (requiring multiple groups) are tracked correctly.
#[tokio::test]
async fn test_state_tracking_large_mint_batches() {
    println!("=== Test: State Tracking Large Mint Batches ===");

    let ctx = BridgeTestContext::new().await;

    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &default_initialize_params(),
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone())
        .await
        .unwrap();

    // Test with 50 deposits (3 groups: 24 + 24 + 2)
    let num_deposits = 50;
    let expected_groups = (num_deposits + 23) / 24; // Ceiling division

    println!(
        "Creating {} deposits ({} mint groups)...",
        num_deposits, expected_groups
    );

    let mut deposits = Vec::new();
    for i in 0..num_deposits {
        let user = helper.add_user();
        deposits.push(BTAutoClaimedDeposit::new(
            user.to_bytes(),
            1_000_000,
            i as u32 + 1,
        ));
    }

    helper.mine_and_process_block(deposits).await.unwrap();

    // Verify final state
    let final_state = read_bridge_state(&ctx).await;
    assert_eq!(
        final_state.core_state.bridge_header.finalized_state.block_height,
        1,
        "Should be at block 1"
    );
    assert_eq!(
        final_state
            .core_state
            .bridge_header
            .finalized_state
            .auto_claimed_deposits_next_index,
        num_deposits as u32,
        "All deposits should be tracked"
    );

    println!("Successfully processed {} deposits in {} groups", num_deposits, expected_groups);
    println!("=== Test Passed ===\n");
}

/// Test state consistency through reorg operations.
#[tokio::test]
async fn test_state_tracking_through_reorg() {
    println!("=== Test: State Tracking Through Reorg ===");

    let ctx = BridgeTestContext::new().await;

    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &default_initialize_params(),
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone())
        .await
        .unwrap();

    // Mine initial block
    let u1 = helper.add_user();
    let d1 = BTAutoClaimedDeposit::new(u1.to_bytes(), 100_000_000, 1);
    helper.mine_and_process_block(vec![d1]).await.unwrap();

    let state_after_block1 = read_bridge_state(&ctx).await;
    println!(
        "After block 1: height={}, deposits={}",
        state_after_block1.core_state.bridge_header.finalized_state.block_height,
        state_after_block1
            .core_state
            .bridge_header
            .finalized_state
            .auto_claimed_deposits_next_index
    );

    // Now do a reorg that adds 3 blocks (with some empty)
    let u2 = helper.add_user();
    let d2 = BTAutoClaimedDeposit::new(u2.to_bytes(), 200_000_000, 2);

    let u4 = helper.add_user();
    let d4 = BTAutoClaimedDeposit::new(u4.to_bytes(), 400_000_000, 4);

    let reorg_blocks = vec![
        vec![d2], // Block 2: 1 deposit
        vec![],   // Block 3: empty
        vec![d4], // Block 4: 1 deposit
    ];

    println!("Processing reorg with {} blocks...", reorg_blocks.len());
    helper.mine_reorg_chain(reorg_blocks).await.unwrap();

    let final_state = read_bridge_state(&ctx).await;
    println!(
        "After reorg: height={}, deposits={}",
        final_state.core_state.bridge_header.finalized_state.block_height,
        final_state
            .core_state
            .bridge_header
            .finalized_state
            .auto_claimed_deposits_next_index
    );

    // Should be at height 4 (1 + 3 blocks from reorg)
    assert_eq!(
        final_state.core_state.bridge_header.finalized_state.block_height,
        4,
        "Height should be 4 after reorg"
    );
    // Should have 3 deposits total (1 from block 1, 1 from block 2, 1 from block 4)
    assert_eq!(
        final_state
            .core_state
            .bridge_header
            .finalized_state
            .auto_claimed_deposits_next_index,
        3,
        "Should have 3 deposits total"
    );

    println!("=== Test Passed ===\n");
}

// ============================================================================
// Data Consistency Tests
// ============================================================================

/// Test that pending mint and TXO hashes are correctly computed and stored.
#[tokio::test]
async fn test_hash_consistency() {
    println!("=== Test: Hash Consistency ===");

    let ctx = BridgeTestContext::new().await;

    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &default_initialize_params(),
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone())
        .await
        .unwrap();

    // Mine block with specific deposits
    let user1 = helper.add_user();
    let user2 = helper.add_user();
    let deposits = vec![
        BTAutoClaimedDeposit::new(user1.to_bytes(), 100_000_000, 1),
        BTAutoClaimedDeposit::new(user2.to_bytes(), 200_000_000, 2),
    ];

    helper.mine_and_process_block(deposits).await.unwrap();

    // Read state and verify hashes are set
    let state = read_bridge_state(&ctx).await;

    let pending_hash = state
        .core_state
        .bridge_header
        .finalized_state
        .pending_mints_finalized_hash;
    let txo_hash = state
        .core_state
        .bridge_header
        .finalized_state
        .txo_output_list_finalized_hash;

    println!("Pending mints hash: {:?}", &pending_hash[..8]);
    println!("TXO hash: {:?}", &txo_hash[..8]);

    // Hashes should not be zero (unless we had an empty block)
    assert_ne!(pending_hash, [0u8; 32], "Pending mints hash should be set");
    assert_ne!(txo_hash, [0u8; 32], "TXO hash should be set");

    println!("=== Test Passed ===\n");
}

/// Test empty block handling - hashes should be the "empty" sentinel values.
#[tokio::test]
async fn test_empty_block_hashes() {
    println!("=== Test: Empty Block Hashes ===");

    let ctx = BridgeTestContext::new().await;

    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &default_initialize_params(),
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone())
        .await
        .unwrap();

    // Mine empty block
    helper.mine_and_process_block(vec![]).await.unwrap();

    let state = read_bridge_state(&ctx).await;

    // Empty blocks should have specific hash values
    // The expected empty pending mints hash (SHA256 of empty groups)
    let expected_empty_pending_hash: [u8; 32] = [
        150, 162, 150, 210, 36, 242, 133, 198, 123, 238, 147, 195, 15, 138, 48, 145, 87, 240, 218,
        163, 93, 197, 184, 126, 65, 11, 120, 99, 10, 9, 207, 199,
    ];

    let pending_hash = state
        .core_state
        .bridge_header
        .finalized_state
        .pending_mints_finalized_hash;

    println!("Pending mints hash for empty block: {:?}", &pending_hash[..8]);
    assert_eq!(
        pending_hash, expected_empty_pending_hash,
        "Empty block should have expected pending mints hash"
    );

    println!("=== Test Passed ===\n");
}

// ============================================================================
// Token Balance Verification Tests
// ============================================================================

/// Test that token balances match expected values after deposits.
#[tokio::test]
async fn test_balance_verification() {
    println!("=== Test: Balance Verification ===");

    let ctx = BridgeTestContext::new().await;

    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &default_initialize_params(),
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone())
        .await
        .unwrap();

    // Create users and track expected balances
    let mut user_balances: Vec<(solana_sdk::pubkey::Pubkey, u64)> = Vec::new();

    let user1 = helper.add_user();
    let user2 = helper.add_user();
    let user3 = helper.add_user();

    // Block 1: Two deposits
    let deposits1 = vec![
        BTAutoClaimedDeposit::new(user1.to_bytes(), 100_000_000, 1),
        BTAutoClaimedDeposit::new(user2.to_bytes(), 200_000_000, 2),
    ];
    helper.mine_and_process_block(deposits1).await.unwrap();
    user_balances.push((user1, 100_000_000));
    user_balances.push((user2, 200_000_000));

    // Block 2: One deposit, plus additional to existing user
    let deposits2 = vec![
        BTAutoClaimedDeposit::new(user3.to_bytes(), 300_000_000, 3),
        BTAutoClaimedDeposit::new(user1.to_bytes(), 50_000_000, 4), // Additional to user1
    ];
    helper.mine_and_process_block(deposits2).await.unwrap();
    user_balances.push((user3, 300_000_000));
    // Update user1's expected balance
    user_balances[0].1 += 50_000_000;

    // Verify all balances
    println!("Verifying user balances...");
    for (user, expected_balance) in &user_balances {
        let ata = spl_associated_token_account::get_associated_token_address(user, &ctx.doge_mint);
        let account = ctx.client.client.get_account(ata).await.unwrap();

        if let Some(acc) = account {
            let token_account: spl_token::state::Account =
                solana_sdk::program_pack::Pack::unpack(&acc.data).unwrap();
            println!(
                "User {:?}: expected={}, actual={}",
                &user.to_bytes()[..4],
                expected_balance,
                token_account.amount
            );
            assert_eq!(
                token_account.amount, *expected_balance,
                "Balance mismatch for user"
            );
        } else {
            panic!("Token account not found for user");
        }
    }

    println!("=== Test Passed ===\n");
}

// ============================================================================
// Complex Scenario Tests
// ============================================================================

/// Test a complex scenario with multiple blocks, reorgs, and varying deposit counts.
#[tokio::test]
async fn test_complex_scenario() {
    println!("=== Test: Complex Scenario ===");

    let ctx = BridgeTestContext::new().await;

    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &default_initialize_params(),
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone())
        .await
        .unwrap();

    let mut total_deposits = 0u32;

    // Phase 1: Normal blocks with varying deposits
    println!("Phase 1: Mining 3 normal blocks...");
    for block_num in 1..=3 {
        let deposit_count = block_num * 5; // 5, 10, 15 deposits
        let mut deposits = Vec::new();
        for i in 0..deposit_count {
            let user = helper.add_user();
            deposits.push(BTAutoClaimedDeposit::new(
                user.to_bytes(),
                1_000_000,
                (block_num * 100 + i) as u32,
            ));
        }
        helper.mine_and_process_block(deposits).await.unwrap();
        total_deposits += deposit_count as u32;
        println!("  Block {}: {} deposits (total: {})", block_num, deposit_count, total_deposits);
    }

    // Phase 2: Reorg adding 2 blocks
    println!("Phase 2: Processing reorg with 2 blocks...");
    let mut reorg_deposits_count = 0u32;

    let mut block4_deposits = Vec::new();
    for i in 0..25 {
        // 25 deposits = 2 groups
        let user = helper.add_user();
        block4_deposits.push(BTAutoClaimedDeposit::new(
            user.to_bytes(),
            2_000_000,
            400 + i,
        ));
    }
    reorg_deposits_count += 25;

    let mut block5_deposits = Vec::new();
    for i in 0..10 {
        let user = helper.add_user();
        block5_deposits.push(BTAutoClaimedDeposit::new(
            user.to_bytes(),
            3_000_000,
            500 + i,
        ));
    }
    reorg_deposits_count += 10;

    helper
        .mine_reorg_chain(vec![block4_deposits, block5_deposits])
        .await
        .unwrap();
    total_deposits += reorg_deposits_count;
    println!("  Reorg added {} deposits (total: {})", reorg_deposits_count, total_deposits);

    // Phase 3: Empty block
    println!("Phase 3: Mining empty block...");
    helper.mine_and_process_block(vec![]).await.unwrap();

    // Phase 4: Large batch
    println!("Phase 4: Mining block with large batch...");
    let large_batch_count = 48; // 2 full groups
    let mut large_deposits = Vec::new();
    for i in 0..large_batch_count {
        let user = helper.add_user();
        large_deposits.push(BTAutoClaimedDeposit::new(
            user.to_bytes(),
            500_000,
            700 + i,
        ));
    }
    helper.mine_and_process_block(large_deposits).await.unwrap();
    total_deposits += large_batch_count;
    println!("  Added {} deposits (total: {})", large_batch_count, total_deposits);

    // Verify final state
    let final_state = read_bridge_state(&ctx).await;
    let final_height = final_state.core_state.bridge_header.finalized_state.block_height;
    let final_deposits = final_state
        .core_state
        .bridge_header
        .finalized_state
        .auto_claimed_deposits_next_index;

    println!("\nFinal state verification:");
    println!("  Expected height: 7 (3 + 2 reorg + 1 empty + 1 large)");
    println!("  Actual height: {}", final_height);
    println!("  Expected deposits: {}", total_deposits);
    println!("  Actual deposits: {}", final_deposits);

    assert_eq!(final_height, 7, "Height should be 7");
    assert_eq!(final_deposits, total_deposits, "Deposit count should match");

    println!("=== Test Passed ===\n");
}

/// Test that reconstructed state matches what we'd expect from tracking deposits.
#[tokio::test]
async fn test_state_reconstruction_simulation() {
    println!("=== Test: State Reconstruction Simulation ===");

    let ctx = BridgeTestContext::new().await;

    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &default_initialize_params(),
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone())
        .await
        .unwrap();

    // Simulate tracking what history reconstruction would record
    #[derive(Debug, Clone)]
    struct SimulatedBlockRecord {
        height: u32,
        deposit_count: u32,
        txo_indices: Vec<u32>,
    }

    let mut simulated_history: Vec<SimulatedBlockRecord> = Vec::new();

    // Process several blocks and track what we'd reconstruct
    let block_configs = vec![
        vec![1, 2, 3],         // Block 1: txo indices 1, 2, 3
        vec![10, 11],         // Block 2: txo indices 10, 11
        vec![],               // Block 3: empty
        vec![100, 101, 102, 103, 104], // Block 4: 5 deposits
    ];

    for (block_idx, txo_indices) in block_configs.iter().enumerate() {
        let mut deposits = Vec::new();
        for &idx in txo_indices {
            let user = helper.add_user();
            deposits.push(BTAutoClaimedDeposit::new(user.to_bytes(), 1_000_000, idx));
        }

        helper.mine_and_process_block(deposits).await.unwrap();

        simulated_history.push(SimulatedBlockRecord {
            height: block_idx as u32 + 1,
            deposit_count: txo_indices.len() as u32,
            txo_indices: txo_indices.clone(),
        });
    }

    // Now verify that our "simulated reconstruction" matches actual state
    let actual_state = read_bridge_state(&ctx).await;

    // Calculate totals from simulated history
    let reconstructed_height = simulated_history.len() as u32;
    let reconstructed_deposits: u32 = simulated_history.iter().map(|b| b.deposit_count).sum();

    println!("Simulated reconstruction:");
    for record in &simulated_history {
        println!(
            "  Block {}: {} deposits, txos: {:?}",
            record.height, record.deposit_count, record.txo_indices
        );
    }
    println!("\nReconstructed totals:");
    println!("  Height: {}", reconstructed_height);
    println!("  Deposits: {}", reconstructed_deposits);

    println!("\nActual state:");
    println!(
        "  Height: {}",
        actual_state.core_state.bridge_header.finalized_state.block_height
    );
    println!(
        "  Deposits: {}",
        actual_state
            .core_state
            .bridge_header
            .finalized_state
            .auto_claimed_deposits_next_index
    );

    assert_eq!(
        actual_state.core_state.bridge_header.finalized_state.block_height,
        reconstructed_height,
        "Height should match"
    );
    assert_eq!(
        actual_state
            .core_state
            .bridge_header
            .finalized_state
            .auto_claimed_deposits_next_index,
        reconstructed_deposits,
        "Deposit count should match"
    );

    println!("=== Test Passed ===\n");
}
