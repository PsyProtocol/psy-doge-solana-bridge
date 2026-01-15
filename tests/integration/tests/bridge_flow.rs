use doge_bridge_client::instructions::{self};
use doge_bridge_test_utils::{
    BridgeTestContext, block_transition_helper::{BTAutoClaimedDeposit, BlockTransitionHelper}, mock_data::*
};
use solana_program_test::tokio;
use solana_sdk::{program_pack::Pack, signature::Signer};
use psy_doge_solana_core::{
    instructions::doge_bridge::InitializeBridgeParams, program_state::{PsyBridgeConfig, PsyReturnTxOutput}
};
use psy_bridge_core::{crypto::hash::sha256::btc_hash256_bytes, custodian_config::BridgeCustodianWalletConfig, header::{PsyBridgeHeader, PsyBridgeStateCommitment, PsyBridgeTipStateCommitment}};
use doge_bridge::state::BridgeState;

#[tokio::test]
async fn test_bridge_extended_flow() {
    let mut ctx = BridgeTestContext::new().await;

    let config_params = PsyBridgeConfig {
        deposit_fee_rate_numerator: 2,
        deposit_fee_rate_denominator: 100,
        withdrawal_fee_rate_numerator: 2,
        withdrawal_fee_rate_denominator: 100,
        deposit_flat_fee_sats: 1000,
        withdrawal_flat_fee_sats: 1000,
    };
    let initialize_params = InitializeBridgeParams {
        bridge_header: PsyBridgeHeader{ tip_state: PsyBridgeTipStateCommitment::default(), finalized_state: PsyBridgeStateCommitment::default(), bridge_state_hash: [0u8; 32], last_rollback_at_secs: 0, paused_until_secs: 0, total_finalized_fees_collected_chain_history: 0 },
        start_return_txo_output: PsyReturnTxOutput { sighash: [0u8; 32], output_index: 0, amount_sats: 0 },
        config_params,
        custodian_wallet_config: BridgeCustodianWalletConfig { wallet_address_hash: [1u8; 20], network_type: 0 },
    };
    
    // Initialize Bridge
    println!("Initializing bridge...");
    let init_ix = instructions::initialize_bridge(ctx.client.payer.pubkey(), ctx.client.operator.pubkey(), ctx.client.fee_spender.pubkey(), ctx.doge_mint, &initialize_params);
    ctx.client.send_tx(&[init_ix], &[]).await;

    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone()).await.unwrap();

    // Mine Empty Block 1
    helper.mine_and_process_block(vec![]).await.unwrap();

    // Mine Block 2 with 2 Users Depositing
    let user1_pk = helper.add_user();
    let user2_pk = helper.add_user();

    let deposit1 = BTAutoClaimedDeposit::new(user1_pk.to_bytes(), 500_000_000, 100);
    let deposit2 = BTAutoClaimedDeposit::new(user2_pk.to_bytes(), 250_000_000, 101);
    // this is triggering an error now for some reason
    helper.mine_and_process_block(vec![deposit1, deposit2]).await.unwrap();

    // Verify Balances
    let user1_ata = spl_associated_token_account::get_associated_token_address(&user1_pk, &ctx.doge_mint);
    let acc1 = ctx.client.client.get_account(user1_ata).await.unwrap().unwrap();
    assert_eq!(spl_token::state::Account::unpack(&acc1.data).unwrap().amount, 500_000_000);

    // Mine Block 3 with Large Deposits (forcing multiple mint groups)
    // Since the MAX_GROUP_SIZE is 24, this creates 2 groups: one with 24 mints and another 6 mints.
    let mut large_batch = Vec::new();
    for i in 0..30 {
        let u = helper.add_user();
        large_batch.push(BTAutoClaimedDeposit::new(u.to_bytes(), 1_000_000_000, 200 + i));
    }
    helper.mine_and_process_block(large_batch).await.unwrap();

    // Withdrawal Flow
    let burn_amount = 400_000_000;
    let user1 = helper.get_user_account(&user1_pk);
    let withdraw_ix = instructions::request_withdrawal(
        ctx.program_id, 
        user1.pubkey(), 
        ctx.doge_mint, 
        user1_ata, 
        [5u8; 20], 
        burn_amount, 
        0
    );
    ctx.client.send_tx(&[withdraw_ix], &[user1]).await;

    // Verify Burn
    let acc1_after = ctx.client.client.get_account(user1_ata).await.unwrap().unwrap();
    assert_eq!(spl_token::state::Account::unpack(&acc1_after.data).unwrap().amount, 100_000_000);

    // Process Withdrawal
    let doge_tx_data = vec![0xEE; 100];
    let doge_tx_hash = btc_hash256_bytes(&doge_tx_data);
    let buffer_pk = ctx.client.create_generic_buffer(&doge_tx_data).await;

    let bridge_account = ctx.client.client.get_account(ctx.client.bridge_state_pda).await.unwrap().unwrap();
    let bridge_state: &BridgeState = bytemuck::from_bytes(&bridge_account.data);
    
    let new_return_output = PsyReturnTxOutput { sighash: doge_tx_hash, output_index: 0, amount_sats: burn_amount };
    let new_spent_root = [99u8; 32];
    let new_index = bridge_state.core_state.next_processed_withdrawals_index + 1;

    let pub_inputs = bridge_state.core_state.get_expected_public_inputs_for_withdrawal_proof(&doge_tx_hash, &new_return_output, new_spent_root, new_index);
    let proof = generate_withdrawal_fake_proof(pub_inputs);

    let fake_wormhole_shim_id = ctx.client.generic_buffer_program_id;
    let fake_wormhole_core_id = ctx.client.generic_buffer_program_id;
    let process_ix = instructions::process_withdrawal(ctx.program_id, ctx.client.payer.pubkey(), buffer_pk, fake_wormhole_shim_id, fake_wormhole_core_id, proof, new_return_output, new_spent_root, new_index);
    ctx.client.send_tx(&[process_ix], &[]).await;

    // Mine Block 4 (Empty) to ensure bridge continues
    helper.mine_and_process_block(vec![]).await.unwrap();

    println!("Extended Bridge Flow Test Completed Successfully");
}
