use doge_bridge_client::instructions::{self};
use doge_bridge_test_utils::{
    block_transition_helper::{BTAutoClaimedDeposit, BlockTransitionHelper},
    BridgeTestContext,
};
use psy_bridge_core::
    header::{PsyBridgeHeader, PsyBridgeStateCommitment}
;
use psy_doge_solana_core::{
    instructions::doge_bridge::InitializeBridgeParams,
    program_state::{PsyBridgeConfig, PsyReturnTxOutput},
};
use solana_program_test::tokio;
use solana_sdk::{program_pack::Pack, signature::Signer};

#[tokio::test]
async fn test_reorg_with_fast_forward() {
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
        bridge_header: PsyBridgeHeader {
            tip_state: PsyBridgeStateCommitment::default(),
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
        config_params,
    };

    // Initialize Bridge
    let init_ix = instructions::initialize_bridge(
        ctx.client.payer.pubkey(),
        ctx.client.operator.pubkey(),
        ctx.client.fee_spender.pubkey(),
        ctx.doge_mint,
        &initialize_params,
    );
    ctx.client.send_tx(&[init_ix], &[]).await;

    let mut helper = BlockTransitionHelper::new_from_client(ctx.client.clone())
        .await
        .unwrap();

    // Mine Normal Block 1
    let u1 = helper.add_user();
    let d1 = BTAutoClaimedDeposit::new(u1.to_bytes(), 500_000_000, 1);
    helper.mine_and_process_block(vec![d1]).await.unwrap();

    // Verify Balances
    let u1_ata = spl_associated_token_account::get_associated_token_address(&u1, &ctx.doge_mint);
    let acc1 = ctx
        .client
        .client
        .get_account(u1_ata)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        spl_token::state::Account::unpack(&acc1.data)
            .unwrap()
            .amount,
        500_000_000
    );

    // Prepare Reorg Scenario
    // We simulate a reorg adding 3 blocks: 2, 3, 4.
    // Block 2: 1 Deposit (Valid)
    // Block 3: Empty (Fast Forward)
    // Block 4: 1 Deposit (Needs JIT Auto Advance)

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
    let acc2 = ctx
        .client
        .client
        .get_account(u2_ata)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        spl_token::state::Account::unpack(&acc2.data)
            .unwrap()
            .amount,
        300_000_000
    );

    // Verify Block 4 Processed (u4 balance) - This proves auto-advance skipped block 3 and processed 4
    let u4_ata = spl_associated_token_account::get_associated_token_address(&u4, &ctx.doge_mint);
    let acc4 = ctx
        .client
        .client
        .get_account(u4_ata)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        spl_token::state::Account::unpack(&acc4.data)
            .unwrap()
            .amount,
        200_000_000
    );

    println!("Reorg with Auto Advance Test Successful");
}
