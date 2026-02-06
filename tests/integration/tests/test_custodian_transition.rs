use doge_bridge_client::instructions;
use doge_bridge_test_utils::{
    block_transition_helper::{BTAutoClaimedDeposit, BlockTransitionHelper},
    BridgeTestContext,
};
use psy_bridge_core::header::{PsyBridgeHeader, PsyBridgeStateCommitment, PsyBridgeTipStateCommitment};
use psy_doge_solana_core::{
    constants::DEPOSITS_PAUSED_MODE_ACTIVE,
    instructions::doge_bridge::InitializeBridgeParams,
    program_state::{PsyBridgeConfig, PsyReturnTxOutput},
};
use solana_program_test::tokio;
use solana_sdk::{program_pack::Pack, signature::Signer};

// Re-export for convenience in tests

fn create_default_config() -> (InitializeBridgeParams, [u8; 32]) {
    let config_params = PsyBridgeConfig {
        deposit_fee_rate_numerator: 2,
        deposit_fee_rate_denominator: 100,
        withdrawal_fee_rate_numerator: 2,
        withdrawal_fee_rate_denominator: 100,
        deposit_flat_fee_sats: 1000,
        withdrawal_flat_fee_sats: 1000,
    };
    let custodian_hash = [1u8; 32];
    let initialize_params = InitializeBridgeParams {
        bridge_header: PsyBridgeHeader {
            tip_state: PsyBridgeTipStateCommitment::default(),
            finalized_state: PsyBridgeStateCommitment::default(),
            bridge_state_hash: [0u8; 32],
            last_rollback_at_secs: 0,
            paused_until_secs: 0,
            total_finalized_fees_collected_chain_history: 0,
        },
        custodian_wallet_config_hash: custodian_hash,
        start_return_txo_output: PsyReturnTxOutput {
            sighash: [0u8; 32],
            output_index: 0,
            amount_sats: 0,
        },
        config_params,
    };
    (initialize_params, custodian_hash)
}

#[tokio::test]
async fn test_custodian_transition_notify_and_cancel() {
    let mut ctx = BridgeTestContext::new().await;
    let (initialize_params, _current_hash) = create_default_config();

    // Initialize Bridge
    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &initialize_params,
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    // Create mock manager set accounts (ManagerSetIndex + ManagerSet)
    let (manager_set_index, manager_set, new_custodian_hash) = ctx.create_mock_manager_set(1);

    // Notify custodian config update
    let mut client = ctx.client.clone();
    client.notify_custodian_config_update(manager_set_index, manager_set, new_custodian_hash).await;

    // Verify the transition is pending
    let bridge_account = client.client.get_account(client.bridge_state_pda).await.unwrap().unwrap();
    let bridge_state: &doge_bridge::state::BridgeState = bytemuck::from_bytes(&bridge_account.data);

    // Check that incoming_transition_custodian_config_hash is set
    assert_eq!(bridge_state.core_state.incoming_transition_custodian_config_hash, new_custodian_hash);
    assert!(bridge_state.core_state.last_detected_custodian_transition_seconds > 0);
    assert_eq!(bridge_state.core_state.deposits_paused_mode, DEPOSITS_PAUSED_MODE_ACTIVE);

    // Cancel the transition
    client.cancel_custodian_transition().await;

    // Verify the transition is cancelled - after cancel, incoming_transition_custodian_config_hash
    // is reset to match custodian_wallet_config_hash (not zeroed)
    let bridge_account = client.client.get_account(client.bridge_state_pda).await.unwrap().unwrap();
    let bridge_state: &doge_bridge::state::BridgeState = bytemuck::from_bytes(&bridge_account.data);

    assert_eq!(bridge_state.core_state.last_detected_custodian_transition_seconds, 0);
    // After cancel, incoming hash is reset to current custodian hash
    assert_eq!(
        bridge_state.core_state.incoming_transition_custodian_config_hash,
        bridge_state.core_state.custodian_wallet_config_hash
    );
    assert_eq!(bridge_state.core_state.deposits_paused_mode, DEPOSITS_PAUSED_MODE_ACTIVE);

    println!("Custodian transition notify and cancel test successful");
}

#[tokio::test]
async fn test_custodian_transition_deposits_allowed_during_grace_period() {
    let mut ctx = BridgeTestContext::new().await;
    let (initialize_params, _current_hash) = create_default_config();

    // Initialize Bridge
    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &initialize_params,
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    // Create mock manager set accounts (ManagerSetIndex + ManagerSet)
    let (manager_set_index, manager_set, new_custodian_hash) = ctx.create_mock_manager_set(1);

    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone())
        .await
        .unwrap();

    // Notify custodian config update
    helper.client.notify_custodian_config_update(manager_set_index, manager_set, new_custodian_hash).await;

    // During grace period, deposits should still be allowed
    let u1 = helper.add_user();
    let d1 = BTAutoClaimedDeposit::new(u1.to_bytes(), 500_000_000, 1);

    // This should succeed since we're still in grace period
    let result = helper.mine_and_process_block(vec![d1]).await;
    assert!(result.is_ok(), "Deposits should be allowed during grace period");

    // Verify balance
    let u1_ata = spl_associated_token_account::get_associated_token_address(&u1, &ctx.doge_mint);
    let acc1 = ctx.client.client.get_account(u1_ata).await.unwrap().unwrap();
    assert_eq!(
        spl_token::state::Account::unpack(&acc1.data).unwrap().amount,
        500_000_000
    );

    println!("Deposits allowed during grace period test successful");
}

#[tokio::test]
async fn test_custodian_transition_state_values() {
    let mut ctx = BridgeTestContext::new().await;
    let (initialize_params, current_hash) = create_default_config();

    // Initialize Bridge
    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &initialize_params,
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    // Verify initial state
    let bridge_account = ctx.client.client.get_account(ctx.client.bridge_state_pda).await.unwrap().unwrap();
    let bridge_state: &doge_bridge::state::BridgeState = bytemuck::from_bytes(&bridge_account.data);

    assert_eq!(bridge_state.core_state.custodian_wallet_config_hash, current_hash);
    assert_eq!(bridge_state.core_state.last_detected_custodian_transition_seconds, 0);
    assert_eq!(bridge_state.core_state.incoming_transition_custodian_config_hash, [0u8; 32]);
    assert_eq!(bridge_state.core_state.deposits_paused_mode, DEPOSITS_PAUSED_MODE_ACTIVE);

    // Create mock manager set accounts (ManagerSetIndex + ManagerSet)
    let (manager_set_index, manager_set, new_custodian_hash) = ctx.create_mock_manager_set(1);

    // Notify custodian config update
    let mut client = ctx.client.clone();
    client.notify_custodian_config_update(manager_set_index, manager_set, new_custodian_hash).await;

    // Verify transition pending state
    let bridge_account = client.client.get_account(client.bridge_state_pda).await.unwrap().unwrap();
    let bridge_state: &doge_bridge::state::BridgeState = bytemuck::from_bytes(&bridge_account.data);

    // Current hash should remain unchanged
    assert_eq!(bridge_state.core_state.custodian_wallet_config_hash, current_hash);
    // Incoming hash should be set to the computed hash from the mock config
    assert_eq!(bridge_state.core_state.incoming_transition_custodian_config_hash, new_custodian_hash);
    // Transition timestamp should be set
    assert!(bridge_state.core_state.last_detected_custodian_transition_seconds > 0);
    // Deposits should still be active (grace period)
    assert_eq!(bridge_state.core_state.deposits_paused_mode, DEPOSITS_PAUSED_MODE_ACTIVE);

    println!("Custodian transition state values test successful");
}
