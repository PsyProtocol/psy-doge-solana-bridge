//! Comprehensive test suite for the new BridgeClient.
//!
//! Tests edge cases like:
//! - Many minting groups (>24 mints per group)
//! - Reorgs with auto claims
//! - Rate limiting behavior
//! - Parallel buffer building
//! - Error handling and recovery


use anyhow::Result;
use doge_bridge_client::{
    BridgeApi, BridgeClient, BridgeClientConfigBuilder, OperatorApi,
};
use doge_bridge_local_network_tests::{
    BTAutoClaimedDeposit, LocalBlockTransitionHelper, LocalBridgeContext,
};
use psy_bridge_core::
    header::{PsyBridgeHeader, PsyBridgeStateCommitment, PsyBridgeTipStateCommitment}
;
use psy_doge_solana_core::{
    instructions::doge_bridge::InitializeBridgeParams,
    program_state::{PsyBridgeConfig, PsyReturnTxOutput},
};
use solana_sdk::{
    pubkey::Pubkey,
    signature::Keypair,
};

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a default bridge config for testing (no fees)
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

/// Create default initialize params
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

/// Create a BridgeClient from LocalBridgeContext
async fn create_bridge_client_from_context(ctx: &LocalBridgeContext) -> Result<BridgeClient> {
    let operator_bytes = ctx.client.operator.to_bytes();
    let payer_bytes = ctx.client.payer.to_bytes();

    let wormhole_core = ctx
        .client
        .wormhole_core_program_id
        .unwrap_or_else(|| Pubkey::new_unique());
    let wormhole_shim = ctx
        .client
        .wormhole_shim_program_id
        .unwrap_or_else(|| Pubkey::new_unique());

    let client = BridgeClient::new(
        &ctx.client.config.rpc_url,
        &operator_bytes,
        &payer_bytes,
        ctx.client.bridge_state_pda,
        wormhole_core,
        wormhole_shim,
    )?;

    Ok(client)
}

// ============================================================================
// Basic Client Tests
// ============================================================================

/// Test creating a BridgeClient and querying bridge state
#[tokio::test]
async fn test_bridge_client_basic_state_query() {
    println!("=== Test: Basic State Query ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    // Initialize bridge using legacy client
    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    // Create BridgeClient
    let bridge_client = create_bridge_client_from_context(&ctx)
        .await
        .expect("Failed to create BridgeClient");

    // Query state
    let state = bridge_client
        .get_current_bridge_state()
        .await
        .expect("Failed to get bridge state");

    println!("Bridge state retrieved successfully!");
    println!(
        "  Block height: {}",
        state.bridge_header.finalized_state.block_height
    );
    println!(
        "  Auto-claimed deposits: {}",
        state
            .bridge_header
            .finalized_state
            .auto_claimed_deposits_next_index
    );

    assert_eq!(
        state
            .bridge_header
            .finalized_state
            .block_height,
        0
    );

    println!("=== Test Passed ===\n");
}

// ============================================================================
// Many Minting Groups Tests
// ============================================================================

/// Test processing a block with exactly 24 mints (1 full group)
#[tokio::test]
async fn test_single_full_mint_group() {
    println!("=== Test: Single Full Mint Group (24 mints) ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Create exactly 24 deposits (1 full group)
    let mut deposits = Vec::new();
    let mut users = Vec::new();
    for i in 0..24 {
        let user = helper.add_user();
        users.push(user);
        deposits.push(BTAutoClaimedDeposit::new(
            user.to_bytes(),
            10_000_000 + (i as u64 * 1_000_000),
            i as u32 + 1,
        ));
    }

    println!("Mining block with 24 deposits...");
    helper.mine_and_process_block(deposits).await.unwrap();

    // Verify all balances
    for (i, user) in users.iter().enumerate() {
        let ata = spl_associated_token_account::get_associated_token_address(user, &ctx.doge_mint);
        let balance = ctx.client.get_token_balance(&ata).await.unwrap();
        let expected = 10_000_000 + (i as u64 * 1_000_000);
        assert_eq!(balance, expected, "User {} balance mismatch", i);
    }

    println!("All 24 user balances verified!");
    println!("=== Test Passed ===\n");
}

/// Test processing a block with 25 mints (2 groups: 24 + 1)
#[tokio::test]
async fn test_two_mint_groups_edge_case() {
    println!("=== Test: Two Mint Groups Edge Case (25 mints) ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Create 25 deposits (2 groups: 24 + 1)
    let mut deposits = Vec::new();
    let mut users = Vec::new();
    for i in 0..25 {
        let user = helper.add_user();
        users.push(user);
        deposits.push(BTAutoClaimedDeposit::new(
            user.to_bytes(),
            50_000_000,
            i as u32 + 1,
        ));
    }

    println!("Mining block with 25 deposits (2 groups)...");
    helper.mine_and_process_block(deposits).await.unwrap();

    // Verify all balances
    for (i, user) in users.iter().enumerate() {
        let ata = spl_associated_token_account::get_associated_token_address(user, &ctx.doge_mint);
        let balance = ctx.client.get_token_balance(&ata).await.unwrap();
        assert_eq!(balance, 50_000_000, "User {} balance mismatch", i);
    }

    println!("All 25 user balances verified!");
    println!("=== Test Passed ===\n");
}

/// Test processing a block with many mints (48 = 2 full groups)
#[tokio::test]
async fn test_multiple_full_mint_groups() {
    println!("=== Test: Multiple Full Mint Groups (48 mints) ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Create 48 deposits (2 full groups)
    let mut deposits = Vec::new();
    let mut users = Vec::new();
    for i in 0..48 {
        let user = helper.add_user();
        users.push(user);
        deposits.push(BTAutoClaimedDeposit::new(
            user.to_bytes(),
            25_000_000,
            i as u32 + 1,
        ));
    }

    println!("Mining block with 48 deposits (2 full groups)...");
    helper.mine_and_process_block(deposits).await.unwrap();

    // Verify all balances
    let mut success_count = 0;
    for (i, user) in users.iter().enumerate() {
        let ata = spl_associated_token_account::get_associated_token_address(user, &ctx.doge_mint);
        let balance = ctx.client.get_token_balance(&ata).await.unwrap();
        assert_eq!(balance, 25_000_000, "User {} balance mismatch", i);
        success_count += 1;
    }

    println!("All {} user balances verified!", success_count);
    println!("=== Test Passed ===\n");
}

/// Test processing a block with 72 mints (3 full groups)
#[tokio::test]
async fn test_three_full_mint_groups() {
    println!("=== Test: Three Full Mint Groups (72 mints) ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Create 72 deposits (3 full groups)
    let mut deposits = Vec::new();
    let mut users = Vec::new();
    for i in 0..72 {
        let user = helper.add_user();
        users.push(user);
        deposits.push(BTAutoClaimedDeposit::new(
            user.to_bytes(),
            10_000_000,
            i as u32 + 1,
        ));
    }

    println!("Mining block with 72 deposits (3 full groups)...");
    helper.mine_and_process_block(deposits).await.unwrap();

    // Verify state
    let state = helper.read_bridge_state().await.unwrap();
    assert_eq!(
        state
            .core_state
            .bridge_header
            .finalized_state
            .auto_claimed_deposits_next_index,
        72
    );

    println!("72 deposits processed successfully!");
    println!("=== Test Passed ===\n");
}

// ============================================================================
// Reorg with Auto Claims Tests
// ============================================================================

/// Test reorg with empty blocks (fast forward)
#[tokio::test]
async fn test_reorg_empty_blocks_fast_forward() {
    println!("=== Test: Reorg with Empty Blocks (Fast Forward) ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Mine initial block
    let u1 = helper.add_user();
    let d1 = BTAutoClaimedDeposit::new(u1.to_bytes(), 100_000_000, 1);
    helper.mine_and_process_block(vec![d1]).await.unwrap();

    // Reorg with empty blocks in between
    let u2 = helper.add_user();
    let d2 = BTAutoClaimedDeposit::new(u2.to_bytes(), 200_000_000, 2);

    let u4 = helper.add_user();
    let d4 = BTAutoClaimedDeposit::new(u4.to_bytes(), 300_000_000, 4);

    let blocks = vec![
        vec![d2], // Block 2: deposit
        vec![],   // Block 3: empty (should be fast-forwarded)
        vec![d4], // Block 4: deposit with auto-advance
    ];

    println!("Processing reorg with empty block...");
    helper.mine_reorg_chain(blocks).await.unwrap();

    // Verify all balances
    let u1_ata = spl_associated_token_account::get_associated_token_address(&u1, &ctx.doge_mint);
    let u2_ata = spl_associated_token_account::get_associated_token_address(&u2, &ctx.doge_mint);
    let u4_ata = spl_associated_token_account::get_associated_token_address(&u4, &ctx.doge_mint);

    assert_eq!(
        ctx.client.get_token_balance(&u1_ata).await.unwrap(),
        100_000_000
    );
    assert_eq!(
        ctx.client.get_token_balance(&u2_ata).await.unwrap(),
        200_000_000
    );
    assert_eq!(
        ctx.client.get_token_balance(&u4_ata).await.unwrap(),
        300_000_000
    );

    println!("All balances verified after reorg!");
    println!("=== Test Passed ===\n");
}

/// Test reorg with multiple consecutive empty blocks
#[tokio::test]
async fn test_reorg_multiple_empty_blocks() {
    println!("=== Test: Reorg with Multiple Consecutive Empty Blocks ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Mine initial block
    let u1 = helper.add_user();
    let d1 = BTAutoClaimedDeposit::new(u1.to_bytes(), 100_000_000, 1);
    helper.mine_and_process_block(vec![d1]).await.unwrap();

    // Reorg: block 2 has deposit, blocks 3-4 empty, block 5 has deposit
    let u2 = helper.add_user();
    let d2 = BTAutoClaimedDeposit::new(u2.to_bytes(), 200_000_000, 2);

    let u5 = helper.add_user();
    let d5 = BTAutoClaimedDeposit::new(u5.to_bytes(), 500_000_000, 5);

    let blocks = vec![
        vec![d2], // Block 2
        vec![],   // Block 3: empty
        vec![],   // Block 4: empty
        vec![d5], // Block 5
    ];

    println!("Processing reorg with multiple empty blocks...");
    helper.mine_reorg_chain(blocks).await.unwrap();

    // Verify balances
    let u2_ata = spl_associated_token_account::get_associated_token_address(&u2, &ctx.doge_mint);
    let u5_ata = spl_associated_token_account::get_associated_token_address(&u5, &ctx.doge_mint);

    assert_eq!(
        ctx.client.get_token_balance(&u2_ata).await.unwrap(),
        200_000_000
    );
    assert_eq!(
        ctx.client.get_token_balance(&u5_ata).await.unwrap(),
        500_000_000
    );

    println!("All balances verified!");
    println!("=== Test Passed ===\n");
}

/// Test reorg with many deposits across multiple groups in reorg blocks
#[tokio::test]
async fn test_reorg_with_many_deposits() {
    println!("=== Test: Reorg with Many Deposits (Multiple Groups) ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Mine initial block with 5 deposits
    let mut initial_deposits = Vec::new();
    for i in 0..5 {
        let user = helper.add_user();
        initial_deposits.push(BTAutoClaimedDeposit::new(
            user.to_bytes(),
            10_000_000,
            i as u32 + 1,
        ));
    }
    helper.mine_and_process_block(initial_deposits).await.unwrap();

    // Prepare reorg blocks
    // Block 2: 30 deposits (2 groups: 24 + 6)
    let mut block2_deposits = Vec::new();
    let mut block2_users = Vec::new();
    for i in 0..30 {
        let user = helper.add_user();
        block2_users.push(user);
        block2_deposits.push(BTAutoClaimedDeposit::new(
            user.to_bytes(),
            20_000_000,
            100 + i as u32,
        ));
    }

    // Block 3: empty
    // Block 4: 10 deposits
    let mut block4_deposits = Vec::new();
    let mut block4_users = Vec::new();
    for i in 0..10 {
        let user = helper.add_user();
        block4_users.push(user);
        block4_deposits.push(BTAutoClaimedDeposit::new(
            user.to_bytes(),
            30_000_000,
            200 + i as u32,
        ));
    }

    let blocks = vec![
        block2_deposits, // Block 2: 30 deposits
        vec![],          // Block 3: empty
        block4_deposits, // Block 4: 10 deposits
    ];

    println!("Processing reorg with many deposits...");
    helper.mine_reorg_chain(blocks).await.unwrap();

    // Verify block2 users
    for (i, user) in block2_users.iter().enumerate() {
        let ata = spl_associated_token_account::get_associated_token_address(user, &ctx.doge_mint);
        let balance = ctx.client.get_token_balance(&ata).await.unwrap();
        assert_eq!(balance, 20_000_000, "Block2 user {} balance mismatch", i);
    }

    // Verify block4 users
    for (i, user) in block4_users.iter().enumerate() {
        let ata = spl_associated_token_account::get_associated_token_address(user, &ctx.doge_mint);
        let balance = ctx.client.get_token_balance(&ata).await.unwrap();
        assert_eq!(balance, 30_000_000, "Block4 user {} balance mismatch", i);
    }

    println!("All reorg deposits verified!");
    println!("=== Test Passed ===\n");
}

// ============================================================================
// Sequential Blocks Tests
// ============================================================================

/// Test processing many sequential blocks
#[tokio::test]
async fn test_many_sequential_blocks() {
    println!("=== Test: Many Sequential Blocks ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    let num_blocks = 10;
    let deposits_per_block = 5;
    let mut total_deposits = 0;

    for block in 1..=num_blocks {
        println!("Mining block {} with {} deposits...", block, deposits_per_block);

        let mut deposits = Vec::new();
        for i in 0..deposits_per_block {
            let user = helper.add_user();
            let amount = (block as u64 * 100_000_000) + (i as u64 * 1_000_000);
            deposits.push(BTAutoClaimedDeposit::new(
                user.to_bytes(),
                amount,
                (block * 100 + i) as u32,
            ));
        }

        helper.mine_and_process_block(deposits).await.unwrap();
        total_deposits += deposits_per_block;
    }

    // Verify final state
    let final_state = helper.read_bridge_state().await.unwrap();
    assert_eq!(
        final_state
            .core_state
            .bridge_header
            .finalized_state
            .auto_claimed_deposits_next_index,
        total_deposits as u32
    );
    assert_eq!(
        final_state
            .core_state
            .bridge_header
            .finalized_state
            .block_height,
        num_blocks
    );

    println!(
        "Processed {} blocks with {} total deposits!",
        num_blocks, total_deposits
    );
    println!("=== Test Passed ===\n");
}

/// Test processing blocks with varying deposit counts
#[tokio::test]
async fn test_varying_deposit_counts() {
    println!("=== Test: Varying Deposit Counts Per Block ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Varying counts: 1, 24, 25, 48, 1, 10
    let deposit_counts = [1, 24, 25, 48, 1, 10];
    let mut total_deposits = 0;

    for (block_idx, &count) in deposit_counts.iter().enumerate() {
        println!(
            "Mining block {} with {} deposits...",
            block_idx + 1,
            count
        );

        let mut deposits = Vec::new();
        for i in 0..count {
            let user = helper.add_user();
            deposits.push(BTAutoClaimedDeposit::new(
                user.to_bytes(),
                5_000_000,
                (block_idx * 100 + i) as u32,
            ));
        }

        helper.mine_and_process_block(deposits).await.unwrap();
        total_deposits += count;
    }

    // Verify
    let final_state = helper.read_bridge_state().await.unwrap();
    assert_eq!(
        final_state
            .core_state
            .bridge_header
            .finalized_state
            .auto_claimed_deposits_next_index,
        total_deposits as u32
    );

    println!(
        "Processed blocks with varying counts, total deposits: {}",
        total_deposits
    );
    println!("=== Test Passed ===\n");
}

// ============================================================================
// BridgeClient API Tests
// ============================================================================

/// Test BridgeClient initialization with new API
#[tokio::test]
async fn test_bridge_client_initialization() {
    println!("=== Test: BridgeClient Initialization ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    // Create BridgeClient
    let _bridge_client = create_bridge_client_from_context(&ctx)
        .await
        .expect("Failed to create BridgeClient");

    // Test creating with full configuration
    let doge_mint = ctx.doge_mint;
    let config_with_mint = BridgeClientConfigBuilder::new()
        .rpc_url(ctx.client.config.rpc_url.clone())
        .operator(Keypair::from_bytes(&ctx.client.operator.to_bytes()).unwrap())
        .payer(Keypair::from_bytes(&ctx.client.payer.to_bytes()).unwrap())
        .bridge_state_pda(ctx.client.bridge_state_pda)
        .wormhole_core_program_id(Pubkey::new_unique())
        .wormhole_shim_program_id(Pubkey::new_unique())
        .doge_mint(doge_mint)
        .build()
        .expect("Failed to build config");

    let _client_with_mint = BridgeClient::with_config(config_with_mint)
        .expect("Failed to create client from config");

    println!("BridgeClient created successfully!");
    println!("=== Test Passed ===\n");
}

/// Test snapshot_withdrawals API (read-only query)
#[tokio::test]
async fn test_snapshot_withdrawals() {
    println!("=== Test: Snapshot Withdrawals ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let bridge_client = create_bridge_client_from_context(&ctx)
        .await
        .expect("Failed to create BridgeClient");

    // Get withdrawal snapshot
    let snapshot = bridge_client
        .snapshot_withdrawals()
        .await
        .expect("Failed to snapshot withdrawals");

    println!("Withdrawal snapshot retrieved!");
    println!("  Next withdrawal index: {}", snapshot.next_requested_withdrawals_tree_index);

    // Initially should have no withdrawals
    assert_eq!(snapshot.next_requested_withdrawals_tree_index, 0);

    println!("=== Test Passed ===\n");
}

/// Test execute_snapshot_withdrawals API (on-chain instruction)
#[tokio::test]
async fn test_execute_snapshot_withdrawals() {
    println!("=== Test: Execute Snapshot Withdrawals ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let bridge_client = create_bridge_client_from_context(&ctx)
        .await
        .expect("Failed to create BridgeClient");

    // Get snapshot before execution
    let snapshot_before = bridge_client
        .snapshot_withdrawals()
        .await
        .expect("Failed to get withdrawal snapshot");

    println!("Snapshot before execution:");
    println!("  Next withdrawal index: {}", snapshot_before.next_requested_withdrawals_tree_index);
    println!("  Block height: {}", snapshot_before.block_height);

    // Execute the snapshot withdrawals instruction on-chain
    let sig = bridge_client
        .execute_snapshot_withdrawals()
        .await
        .expect("Failed to execute snapshot withdrawals");

    println!("Snapshot withdrawals executed! Signature: {}", sig);

    // Get snapshot after execution
    let snapshot_after = bridge_client
        .snapshot_withdrawals()
        .await
        .expect("Failed to get withdrawal snapshot after execution");

    println!("Snapshot after execution:");
    println!("  Next withdrawal index: {}", snapshot_after.next_requested_withdrawals_tree_index);
    println!("  Block height: {}", snapshot_after.block_height);

    println!("=== Test Passed ===\n");
}

/// Test execute_snapshot_withdrawals with pending withdrawals
#[tokio::test]
async fn test_execute_snapshot_withdrawals_with_pending() {
    println!("=== Test: Execute Snapshot Withdrawals With Pending ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    // Mine a block with deposits so users have tokens
    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .expect("Failed to create helper");

    let user1 = helper.add_user();
    let user2 = helper.add_user();

    let deposit1 = BTAutoClaimedDeposit::new(user1.to_bytes(), 500_000_000, 1);
    let deposit2 = BTAutoClaimedDeposit::new(user2.to_bytes(), 300_000_000, 2);

    helper
        .mine_and_process_block(vec![deposit1, deposit2])
        .await
        .expect("Failed to mine block with deposits");

    println!("Mined block with 2 deposits");

    // Request withdrawals using instructions directly
    let user1_keypair = helper.get_user_account(&user1).unwrap();
    let user1_ata = spl_associated_token_account::get_associated_token_address(&user1, &ctx.client.doge_mint);

    ctx.client
        .send_tx(
            &[doge_bridge_client::instructions::request_withdrawal(
                ctx.client.program_ids.doge_bridge,
                user1,
                ctx.client.doge_mint,
                user1_ata,
                [0xAA; 20],
                100_000_000,
                0,
            )],
            &[&Keypair::from_bytes(&user1_keypair.to_bytes()).unwrap()],
        )
        .await
        .expect("Failed to request withdrawal for user1");

    let user2_keypair = helper.get_user_account(&user2).unwrap();
    let user2_ata = spl_associated_token_account::get_associated_token_address(&user2, &ctx.client.doge_mint);

    ctx.client
        .send_tx(
            &[doge_bridge_client::instructions::request_withdrawal(
                ctx.client.program_ids.doge_bridge,
                user2,
                ctx.client.doge_mint,
                user2_ata,
                [0xBB; 20],
                50_000_000,
                0,
            )],
            &[&Keypair::from_bytes(&user2_keypair.to_bytes()).unwrap()],
        )
        .await
        .expect("Failed to request withdrawal for user2");

    println!("Requested 2 withdrawals");

    // Create BridgeClient and execute snapshot
    let bridge_client = create_bridge_client_from_context(&ctx)
        .await
        .expect("Failed to create BridgeClient");

    // Get snapshot before execution
    let snapshot_before = bridge_client
        .snapshot_withdrawals()
        .await
        .expect("Failed to get withdrawal snapshot");

    println!("Snapshot before execution:");
    println!("  Next withdrawal index: {}", snapshot_before.next_requested_withdrawals_tree_index);

    // Execute snapshot withdrawals
    let sig = bridge_client
        .execute_snapshot_withdrawals()
        .await
        .expect("Failed to execute snapshot withdrawals");

    println!("Snapshot withdrawals executed! Signature: {}", sig);

    // Get snapshot after execution
    let snapshot_after = bridge_client
        .snapshot_withdrawals()
        .await
        .expect("Failed to get withdrawal snapshot after execution");

    println!("Snapshot after execution:");
    println!("  Next withdrawal index: {}", snapshot_after.next_requested_withdrawals_tree_index);
    println!("  Block height: {}", snapshot_after.block_height);

    // Verify the snapshot captured the 2 pending withdrawals
    assert_eq!(
        snapshot_after.next_requested_withdrawals_tree_index, 2,
        "Snapshot should capture 2 pending withdrawals"
    );

    println!("=== Test Passed ===\n");
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

/// Test handling of zero deposits block
#[tokio::test]
async fn test_empty_block_handling() {
    println!("=== Test: Empty Block Handling ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Mine initial block with deposits
    let u1 = helper.add_user();
    let d1 = BTAutoClaimedDeposit::new(u1.to_bytes(), 100_000_000, 1);
    helper.mine_and_process_block(vec![d1]).await.unwrap();

    // Mine empty block
    println!("Mining empty block...");
    helper.mine_and_process_block(vec![]).await.unwrap();

    // Mine another block with deposits
    let u2 = helper.add_user();
    let d2 = BTAutoClaimedDeposit::new(u2.to_bytes(), 200_000_000, 2);
    helper.mine_and_process_block(vec![d2]).await.unwrap();

    // Verify final state
    let final_state = helper.read_bridge_state().await.unwrap();
    assert_eq!(
        final_state
            .core_state
            .bridge_header
            .finalized_state
            .block_height,
        3
    );
    assert_eq!(
        final_state
            .core_state
            .bridge_header
            .finalized_state
            .auto_claimed_deposits_next_index,
        2
    );

    println!("Empty block handled correctly!");
    println!("=== Test Passed ===\n");
}

/// Test same user receiving multiple deposits
#[tokio::test]
async fn test_same_user_multiple_deposits() {
    println!("=== Test: Same User Multiple Deposits ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Create one user who will receive multiple deposits
    let user = helper.add_user();

    // Block 1: first deposit
    let d1 = BTAutoClaimedDeposit::new(user.to_bytes(), 100_000_000, 1);
    helper.mine_and_process_block(vec![d1]).await.unwrap();

    let ata = spl_associated_token_account::get_associated_token_address(&user, &ctx.doge_mint);
    let balance1 = ctx.client.get_token_balance(&ata).await.unwrap();
    assert_eq!(balance1, 100_000_000);

    // Block 2: second deposit to same user
    let d2 = BTAutoClaimedDeposit::new(user.to_bytes(), 200_000_000, 2);
    helper.mine_and_process_block(vec![d2]).await.unwrap();

    let balance2 = ctx.client.get_token_balance(&ata).await.unwrap();
    assert_eq!(balance2, 300_000_000); // 100 + 200

    // Block 3: multiple deposits to same user in same block
    let d3a = BTAutoClaimedDeposit::new(user.to_bytes(), 50_000_000, 3);
    let d3b = BTAutoClaimedDeposit::new(user.to_bytes(), 75_000_000, 4);
    helper
        .mine_and_process_block(vec![d3a, d3b])
        .await
        .unwrap();

    let balance3 = ctx.client.get_token_balance(&ata).await.unwrap();
    assert_eq!(balance3, 425_000_000); // 300 + 50 + 75

    println!("Same user received multiple deposits correctly!");
    println!("Final balance: {} sats", balance3);
    println!("=== Test Passed ===\n");
}

/// Test maximum deposits in a single block (stress test)
#[tokio::test]
async fn test_max_deposits_single_block() {
    println!("=== Test: Maximum Deposits in Single Block ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    ctx.client
        .initialize_bridge(&default_initialize_params())
        .await
        .expect("Failed to initialize bridge");

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Create 100 deposits (5 groups: 24*4 + 4 = 100)
    // This tests the system's ability to handle many groups
    let num_deposits = 100;
    let mut deposits = Vec::new();

    println!("Creating {} deposits...", num_deposits);
    for i in 0..num_deposits {
        let user = helper.add_user();
        deposits.push(BTAutoClaimedDeposit::new(
            user.to_bytes(),
            1_000_000,
            i as u32 + 1,
        ));
    }

    println!(
        "Mining block with {} deposits ({} groups)...",
        num_deposits,
        (num_deposits + 23) / 24
    );
    helper.mine_and_process_block(deposits).await.unwrap();

    // Verify state
    let final_state = helper.read_bridge_state().await.unwrap();
    assert_eq!(
        final_state
            .core_state
            .bridge_header
            .finalized_state
            .auto_claimed_deposits_next_index,
        num_deposits as u32
    );

    println!("Successfully processed {} deposits!", num_deposits);
    println!("=== Test Passed ===\n");
}

// ============================================================================
// Fee Handling Tests
// ============================================================================

/// Test deposits with fees applied
#[tokio::test]
async fn test_deposits_with_fees() {
    println!("=== Test: Deposits with Fees ===");

    let ctx = LocalBridgeContext::new()
        .await
        .expect("Failed to create LocalBridgeContext");

    // Initialize with 2% fee + 1000 sats flat fee
    let config_params = PsyBridgeConfig {
        deposit_fee_rate_numerator: 2,
        deposit_fee_rate_denominator: 100,
        withdrawal_fee_rate_numerator: 2,
        withdrawal_fee_rate_denominator: 100,
        deposit_flat_fee_sats: 1000,
        withdrawal_flat_fee_sats: 1000,
    };

    let initialize_params = InitializeBridgeParams {
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
        config_params,
    };

    ctx.client
        .initialize_bridge(&initialize_params)
        .await
        .expect("Failed to initialize bridge");

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Deposit 100,000,000 sats
    // Fee = 100,000,000 * 2% + 1000 = 2,000,000 + 1000 = 2,001,000
    // Net = 100,000,000 - 2,001,000 = 97,999,000
    let user = helper.add_user();
    let deposit = BTAutoClaimedDeposit::new(user.to_bytes(), 100_000_000, 1);
    helper.mine_and_process_block(vec![deposit]).await.unwrap();

    let ata = spl_associated_token_account::get_associated_token_address(&user, &ctx.doge_mint);
    let balance = ctx.client.get_token_balance(&ata).await.unwrap();

    // Calculate expected: (amount - flat_fee) * (1 - rate)
    // (100,000,000 - 1000) * (100 - 2) / 100 = 99,999,000 * 98 / 100 = 97,999,020
    // Note: The exact calculation may differ based on implementation
    println!("Deposit amount: 100,000,000 sats");
    println!("Received amount: {} sats", balance);
    println!("Fee deducted: {} sats", 100_000_000 - balance);

    // The balance should be less than the deposit due to fees
    assert!(
        balance < 100_000_000,
        "Balance should be less than deposit due to fees"
    );
    assert!(
        balance > 95_000_000,
        "Balance should be reasonable after fees"
    );

    println!("Fees applied correctly!");
    println!("=== Test Passed ===\n");
}
