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

/// Test a reorg scenario with fast forward (skipping empty blocks)
#[tokio::test]
async fn test_reorg_with_fast_forward() {
    // Print program IDs for debugging
    doge_bridge_local_network_tests::print_program_ids().unwrap();

    // Create test context
    let ctx = LocalBridgeContext::new().await
        .expect("Failed to create LocalBridgeContext. Is the validator running and programs deployed?");

    // Initialize bridge parameters
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

    // Initialize Bridge
    println!("Initializing bridge...");
    ctx.client.initialize_bridge(&initialize_params).await
        .expect("Failed to initialize bridge");
    println!("Bridge initialized successfully!");

    // Create block transition helper
    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Mine Normal Block 1
    println!("\n=== Mining Block 1 ===");
    let u1 = helper.add_user();
    let d1 = BTAutoClaimedDeposit::new(u1.to_bytes(), 500_000_000, 1);
    helper.mine_and_process_block(vec![d1]).await.unwrap();

    // Verify Balances
    let u1_ata = spl_associated_token_account::get_associated_token_address(&u1, &ctx.doge_mint);
    let u1_balance = ctx.client.get_token_balance(&u1_ata).await.unwrap();
    assert_eq!(u1_balance, 500_000_000, "User 1 balance mismatch after block 1");
    println!("User 1 balance verified: {} sats", u1_balance);

    // Prepare Reorg Scenario
    // We simulate a reorg adding 3 blocks: 2, 3, 4.
    // Block 2: 1 Deposit (Valid)
    // Block 3: Empty (Fast Forward)
    // Block 4: 1 Deposit (Needs JIT Auto Advance)
    println!("\n=== Preparing Reorg Scenario ===");

    let u2 = helper.add_user();
    let d2 = BTAutoClaimedDeposit::new(u2.to_bytes(), 300_000_000, 2);

    let u4 = helper.add_user();
    let d4 = BTAutoClaimedDeposit::new(u4.to_bytes(), 200_000_000, 4);

    let blocks = vec![
        vec![d2], // Block 2
        vec![],   // Block 3 (Empty)
        vec![d4], // Block 4
    ];

    println!("Starting Reorg Sequence...");
    helper.mine_reorg_chain(blocks).await.unwrap();

    // Verify Block 2 Processed (u2 balance)
    let u2_ata = spl_associated_token_account::get_associated_token_address(&u2, &ctx.doge_mint);
    let u2_balance = ctx.client.get_token_balance(&u2_ata).await.unwrap();
    assert_eq!(u2_balance, 300_000_000, "User 2 balance mismatch after reorg");
    println!("User 2 balance verified: {} sats", u2_balance);

    // Verify Block 4 Processed (u4 balance) - This proves auto-advance skipped block 3 and processed 4
    let u4_ata = spl_associated_token_account::get_associated_token_address(&u4, &ctx.doge_mint);
    let u4_balance = ctx.client.get_token_balance(&u4_ata).await.unwrap();
    assert_eq!(u4_balance, 200_000_000, "User 4 balance mismatch after reorg");
    println!("User 4 balance verified: {} sats", u4_balance);

    println!("\n=== Reorg with Auto Advance Test Successful ===");
}

/// Test basic deposit flow without reorg
#[tokio::test]
async fn test_basic_deposit_flow() {
    let ctx = LocalBridgeContext::new().await
        .expect("Failed to create LocalBridgeContext");

    let config_params = PsyBridgeConfig {
        deposit_fee_rate_numerator: 0,
        deposit_fee_rate_denominator: 100,
        withdrawal_fee_rate_numerator: 0,
        withdrawal_fee_rate_denominator: 100,
        deposit_flat_fee_sats: 0,
        withdrawal_flat_fee_sats: 0,
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

    println!("Initializing bridge for basic deposit test...");
    ctx.client.initialize_bridge(&initialize_params).await.unwrap();

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Create 3 users with deposits
    println!("\n=== Mining Block with 3 Deposits ===");
    let u1 = helper.add_user();
    let u2 = helper.add_user();
    let u3 = helper.add_user();

    let deposits = vec![
        BTAutoClaimedDeposit::new(u1.to_bytes(), 100_000_000, 1),
        BTAutoClaimedDeposit::new(u2.to_bytes(), 200_000_000, 2),
        BTAutoClaimedDeposit::new(u3.to_bytes(), 300_000_000, 3),
    ];

    helper.mine_and_process_block(deposits).await.unwrap();

    // Verify all balances
    let u1_ata = spl_associated_token_account::get_associated_token_address(&u1, &ctx.doge_mint);
    let u2_ata = spl_associated_token_account::get_associated_token_address(&u2, &ctx.doge_mint);
    let u3_ata = spl_associated_token_account::get_associated_token_address(&u3, &ctx.doge_mint);

    assert_eq!(ctx.client.get_token_balance(&u1_ata).await.unwrap(), 100_000_000);
    assert_eq!(ctx.client.get_token_balance(&u2_ata).await.unwrap(), 200_000_000);
    assert_eq!(ctx.client.get_token_balance(&u3_ata).await.unwrap(), 300_000_000);

    println!("All balances verified!");
    println!("\n=== Basic Deposit Flow Test Successful ===");
}

/// Test multiple blocks in sequence
#[tokio::test]
async fn test_multiple_blocks() {
    let ctx = LocalBridgeContext::new().await
        .expect("Failed to create LocalBridgeContext");

    let config_params = PsyBridgeConfig {
        deposit_fee_rate_numerator: 0,
        deposit_fee_rate_denominator: 100,
        withdrawal_fee_rate_numerator: 0,
        withdrawal_fee_rate_denominator: 100,
        deposit_flat_fee_sats: 0,
        withdrawal_flat_fee_sats: 0,
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

    ctx.client.initialize_bridge(&initialize_params).await.unwrap();

    let mut helper = LocalBlockTransitionHelper::new_from_client(ctx.client.try_clone().unwrap())
        .await
        .unwrap();

    // Mine 5 blocks with varying numbers of deposits
    for block_num in 1..=5 {
        println!("\n=== Mining Block {} ===", block_num);

        let mut deposits = Vec::new();
        for i in 0..block_num {
            let user = helper.add_user();
            let amount = (block_num as u64 * 100_000_000) + (i as u64 * 10_000_000);
            deposits.push(BTAutoClaimedDeposit::new(
                user.to_bytes(),
                amount,
                (block_num * 10 + i) as u32,
            ));
        }

        helper.mine_and_process_block(deposits).await.unwrap();
    }

    // Read final bridge state
    let final_state = helper.read_bridge_state().await.unwrap();
    println!("\nFinal bridge state:");
    println!("  Block height: {}", final_state.core_state.bridge_header.finalized_state.block_height);
    println!("  Total auto-claimed deposits: {}", final_state.core_state.bridge_header.finalized_state.auto_claimed_deposits_next_index);

    // Expected: 1+2+3+4+5 = 15 total deposits
    assert_eq!(
        final_state.core_state.bridge_header.finalized_state.auto_claimed_deposits_next_index,
        15,
        "Total deposits mismatch"
    );

    println!("\n=== Multiple Blocks Test Successful ===");
}
