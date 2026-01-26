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

fn default_bridge_config() -> PsyBridgeConfig {
    PsyBridgeConfig {
        deposit_fee_rate_numerator: 1,
        deposit_fee_rate_denominator: 100,
        withdrawal_fee_rate_numerator: 1,
        withdrawal_fee_rate_denominator: 100,
        deposit_flat_fee_sats: 1000,
        withdrawal_flat_fee_sats: 1000,
    }
}

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
        start_return_txo_output: PsyReturnTxOutput {
            sighash: [0u8; 32],
            output_index: 0,
            amount_sats: 0,
        },
        config_params: default_bridge_config(),
        custodian_wallet_config_hash: [1u8; 32],
    }
}

/// Test that snapshot_withdrawals instruction works correctly
#[tokio::test]
async fn test_snapshot_withdrawals_basic() {
    let ctx = BridgeTestContext::new().await;

    // Initialize bridge
    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &default_initialize_params(),
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    // Read initial state
    let bridge_account = ctx
        .client
        .client
        .get_account(ctx.client.bridge_state_pda)
        .await
        .unwrap()
        .unwrap();
    let bridge_state: &BridgeState = bytemuck::from_bytes(&bridge_account.data);
    let initial_snapshot = bridge_state.core_state.withdrawal_snapshot;

    // Initially, snapshot should be empty
    assert_eq!(initial_snapshot.next_requested_withdrawals_tree_index, 0);
    assert_eq!(initial_snapshot.block_height, 0);

    // Execute snapshot_withdrawals instruction
    let snapshot_ix = instructions::snapshot_withdrawals(
        ctx.program_id,
        ctx.client.operator.pubkey(),
        ctx.client.payer.pubkey(),
    );
    ctx.client.send_tx(&[snapshot_ix], &[&ctx.client.operator]).await;

    // Read state after snapshot
    let bridge_account_after = ctx
        .client
        .client
        .get_account(ctx.client.bridge_state_pda)
        .await
        .unwrap()
        .unwrap();
    let bridge_state_after: &BridgeState = bytemuck::from_bytes(&bridge_account_after.data);
    let snapshot_after = bridge_state_after.core_state.withdrawal_snapshot;

    // Snapshot should now have a timestamp (block_height captures current state)
    println!("Snapshot after execution:");
    println!(
        "  next_requested_withdrawals_tree_index: {}",
        snapshot_after.next_requested_withdrawals_tree_index
    );
    println!("  block_height: {}", snapshot_after.block_height);

    println!("test_snapshot_withdrawals_basic completed successfully");
}

/// Test snapshot_withdrawals after withdrawal requests
#[tokio::test]
async fn test_snapshot_withdrawals_with_pending_withdrawals() {
    let ctx = BridgeTestContext::new().await;

    // Initialize bridge
    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &default_initialize_params(),
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    // Create helper and mine a block with deposits
    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone())
        .await
        .unwrap();

    let user1_pk = helper.add_user();
    let user2_pk = helper.add_user();

    let deposit1 = BTAutoClaimedDeposit::new(user1_pk.to_bytes(), 500_000_000, 100);
    let deposit2 = BTAutoClaimedDeposit::new(user2_pk.to_bytes(), 300_000_000, 101);
    helper
        .mine_and_process_block(vec![deposit1, deposit2])
        .await
        .unwrap();

    // User1 requests a withdrawal
    let user1 = helper.get_user_account(&user1_pk);
    let user1_ata =
        spl_associated_token_account::get_associated_token_address(&user1_pk, &ctx.doge_mint);

    let withdraw_ix = instructions::request_withdrawal(
        ctx.program_id,
        user1.pubkey(),
        ctx.doge_mint,
        user1_ata,
        [0xAB; 20], // Dogecoin recipient address
        100_000_000,
        0, // P2PKH address type
    );
    ctx.client.send_tx(&[withdraw_ix], &[user1]).await;

    // User2 requests a withdrawal
    let user2 = helper.get_user_account(&user2_pk);
    let user2_ata =
        spl_associated_token_account::get_associated_token_address(&user2_pk, &ctx.doge_mint);

    let withdraw_ix2 = instructions::request_withdrawal(
        ctx.program_id,
        user2.pubkey(),
        ctx.doge_mint,
        user2_ata,
        [0xCD; 20],
        50_000_000,
        0,
    );
    ctx.client.send_tx(&[withdraw_ix2], &[user2]).await;

    // Read state before snapshot
    let bridge_account = ctx
        .client
        .client
        .get_account(ctx.client.bridge_state_pda)
        .await
        .unwrap()
        .unwrap();
    let bridge_state: &BridgeState = bytemuck::from_bytes(&bridge_account.data);

    // Should have 2 pending withdrawals (check the actual tree, not the snapshot)
    assert_eq!(
        bridge_state
            .core_state
            .requested_withdrawals_tree
            .next_index,
        2,
        "Should have 2 pending withdrawal requests"
    );

    // Execute snapshot_withdrawals
    let snapshot_ix = instructions::snapshot_withdrawals(
        ctx.program_id,
        ctx.client.operator.pubkey(),
        ctx.client.payer.pubkey(),
    );
    ctx.client.send_tx(&[snapshot_ix], &[&ctx.client.operator]).await;

    // Read state after snapshot
    let bridge_account_after = ctx
        .client
        .client
        .get_account(ctx.client.bridge_state_pda)
        .await
        .unwrap()
        .unwrap();
    let bridge_state_after: &BridgeState = bytemuck::from_bytes(&bridge_account_after.data);
    let snapshot = bridge_state_after.core_state.withdrawal_snapshot;

    // Snapshot should capture the pending withdrawals
    assert_eq!(
        snapshot.next_requested_withdrawals_tree_index, 2,
        "Snapshot should capture 2 pending withdrawals"
    );

    println!("Snapshot after withdrawals:");
    println!(
        "  next_requested_withdrawals_tree_index: {}",
        snapshot.next_requested_withdrawals_tree_index
    );
    println!("  block_height: {}", snapshot.block_height);

    println!("test_snapshot_withdrawals_with_pending_withdrawals completed successfully");
}

/// Test that only the operator can call snapshot_withdrawals
#[tokio::test]
#[should_panic(expected = "MissingRequiredSignature")]
async fn test_snapshot_withdrawals_requires_operator() {
    use solana_sdk::system_instruction;

    let ctx = BridgeTestContext::new().await;

    // Initialize bridge
    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &default_initialize_params(),
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    // Create a new random keypair that is NOT the operator
    let wrong_operator = solana_sdk::signature::Keypair::new();

    // Fund the wrong_operator so they can sign
    let fund_ix = system_instruction::transfer(
        &ctx.client.payer.pubkey(),
        &wrong_operator.pubkey(),
        1_000_000_000, // 1 SOL
    );
    ctx.client.send_tx(&[fund_ix], &[]).await;

    // Try to call snapshot_withdrawals with wrong operator
    let snapshot_ix = instructions::snapshot_withdrawals(
        ctx.program_id,
        wrong_operator.pubkey(), // Wrong signer - not the operator
        ctx.client.payer.pubkey(),
    );

    // This should fail because wrong_operator is not the operator stored in bridge state
    ctx.client
        .send_tx(&[snapshot_ix], &[&wrong_operator])
        .await;
}

/// Test multiple sequential snapshots
#[tokio::test]
async fn test_multiple_snapshots() {
    let ctx = BridgeTestContext::new().await;

    // Initialize bridge
    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &default_initialize_params(),
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    // Create helper and process deposits
    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone())
        .await
        .unwrap();

    let user1_pk = helper.add_user();
    let deposit1 = BTAutoClaimedDeposit::new(user1_pk.to_bytes(), 1_000_000_000, 100);
    helper.mine_and_process_block(vec![deposit1]).await.unwrap();

    // First withdrawal request
    let user1 = helper.get_user_account(&user1_pk);
    let user1_ata =
        spl_associated_token_account::get_associated_token_address(&user1_pk, &ctx.doge_mint);

    let withdraw_ix = instructions::request_withdrawal(
        ctx.program_id,
        user1.pubkey(),
        ctx.doge_mint,
        user1_ata,
        [0x11; 20],
        100_000_000,
        0,
    );
    ctx.client.send_tx(&[withdraw_ix], &[user1]).await;

    // First snapshot
    let snapshot_ix1 = instructions::snapshot_withdrawals(
        ctx.program_id,
        ctx.client.operator.pubkey(),
        ctx.client.payer.pubkey(),
    );
    ctx.client.send_tx(&[snapshot_ix1], &[&ctx.client.operator]).await;

    let bridge_account1 = ctx
        .client
        .client
        .get_account(ctx.client.bridge_state_pda)
        .await
        .unwrap()
        .unwrap();
    let bridge_state1: &BridgeState = bytemuck::from_bytes(&bridge_account1.data);
    let snapshot1 = bridge_state1.core_state.withdrawal_snapshot;

    assert_eq!(snapshot1.next_requested_withdrawals_tree_index, 1);
    println!("First snapshot: {} withdrawals", snapshot1.next_requested_withdrawals_tree_index);

    // Second withdrawal request
    let user1 = helper.get_user_account(&user1_pk);
    let withdraw_ix2 = instructions::request_withdrawal(
        ctx.program_id,
        user1.pubkey(),
        ctx.doge_mint,
        user1_ata,
        [0x22; 20],
        200_000_000,
        0,
    );
    ctx.client.send_tx(&[withdraw_ix2], &[user1]).await;

    // Second snapshot
    let snapshot_ix2 = instructions::snapshot_withdrawals(
        ctx.program_id,
        ctx.client.operator.pubkey(),
        ctx.client.payer.pubkey(),
    );
    ctx.client.send_tx(&[snapshot_ix2], &[&ctx.client.operator]).await;

    let bridge_account2 = ctx
        .client
        .client
        .get_account(ctx.client.bridge_state_pda)
        .await
        .unwrap()
        .unwrap();
    let bridge_state2: &BridgeState = bytemuck::from_bytes(&bridge_account2.data);
    let snapshot2 = bridge_state2.core_state.withdrawal_snapshot;

    assert_eq!(snapshot2.next_requested_withdrawals_tree_index, 2);
    println!("Second snapshot: {} withdrawals", snapshot2.next_requested_withdrawals_tree_index);

    println!("test_multiple_snapshots completed successfully");
}
