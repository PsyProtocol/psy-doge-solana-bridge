pub mod mock_data;
pub mod test_client;
pub mod block_transition_helper;
pub mod builders;
pub mod tree;
pub mod constants;

// 1. Add these imports at the top of the file.
use doge_bridge::processor::process_instruction as doge_bridge_processor;
use generic_buffer::process_instruction as generic_buffer_processor;
use manual_claim::processor::process_instruction as manual_claim_processor;
use pending_mint_buffer::process_instruction as pending_mint_processor;
use psy_doge_solana_core::programs::{DOGE_BRIDGE_PROGRAM_ID_STR, GENERIC_BUFFER_BUILDER_PROGRAM_ID_STR, MANUAL_CLAIM_PROGRAM_ID_STR, PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID_STR, TXO_BUFFER_BUILDER_PROGRAM_ID_STR};
use solana_program_test::{processor, ProgramTest, ProgramTestContext};
use txo_buffer::process_instruction as txo_buffer_processor;

use solana_sdk::{
    account::Account,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use spl_token::state::Mint;
use solana_program::program_pack::Pack;
use test_client::TestBridgeClient;

pub struct BridgeTestContext {
    pub context: ProgramTestContext,
    pub program_id: Pubkey,
    pub manual_claim_pid: Pubkey,
    pub pending_mint_pid: Pubkey,
    pub txo_buffer_pid: Pubkey,
    pub generic_buffer_pid: Pubkey,
    pub doge_mint: Pubkey,
    pub client: TestBridgeClient,
}

impl BridgeTestContext {
    pub async fn new() -> Self {
        let bridge_pid = Pubkey::from_str_const(DOGE_BRIDGE_PROGRAM_ID_STR);
        let manual_pid = Pubkey::from_str_const(MANUAL_CLAIM_PROGRAM_ID_STR);
        let pending_pid = Pubkey::from_str_const(PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID_STR);
        let txo_pid = Pubkey::from_str_const(TXO_BUFFER_BUILDER_PROGRAM_ID_STR);
        let generic_pid = Pubkey::from_str_const(GENERIC_BUFFER_BUILDER_PROGRAM_ID_STR);

        let mut pt = ProgramTest::default();

        // 2. Replace the old `pt.add_program(..., None)` calls with these lines.
        // This directly links the program logic to the test runner.
        pt.add_program("doge_bridge", bridge_pid, processor!(doge_bridge_processor));
        pt.add_program("manual_claim", manual_pid, processor!(manual_claim_processor));
        pt.add_program("pending_mint_buffer", pending_pid, processor!(pending_mint_processor));
        pt.add_program("txo_buffer", txo_pid, processor!(txo_buffer_processor));
        pt.add_program("generic_buffer", generic_pid, processor!(generic_buffer_processor));

        let context = pt.start_with_context().await;
        
        let doge_mint = Keypair::new();
        let payer_pubkey = context.payer.pubkey();
        let (bridge_pda, _) = Pubkey::find_program_address(&[b"bridge_state"], &bridge_pid);
        println!("bridge_pda: {}", bridge_pda);
        println!("payer_pubkey: {}", payer_pubkey);
        let rent = context.banks_client.get_rent().await.unwrap();
        let min_bal = rent.minimum_balance(Mint::LEN);
        println!("creating mint account...");
        let create_mint_tx = Transaction::new_signed_with_payer(
            &[
                system_instruction::create_account(&payer_pubkey, &doge_mint.pubkey(), min_bal, Mint::LEN as u64, &spl_token::id()),
                spl_token::instruction::initialize_mint(&spl_token::id(), &doge_mint.pubkey(), &bridge_pda, None, 8).unwrap(),
            ],
            Some(&payer_pubkey),
            &[&context.payer, &doge_mint],
            context.last_blockhash,
        );
        context.banks_client.process_transaction(create_mint_tx).await.unwrap();
        println!("created mint!");

        let client_payer = Keypair::from_bytes(&context.payer.to_bytes()).unwrap();
        let client_operator = Keypair::from_bytes(&context.payer.to_bytes()).unwrap();
        let client_fee_spender = Keypair::from_bytes(&context.payer.to_bytes()).unwrap();
        let test_client = TestBridgeClient {
            bridge_state_pda: bridge_pda,
            client: context.banks_client.clone(),
            payer: client_payer,
            operator: client_operator,
            fee_spender: client_fee_spender,
            program_id: bridge_pid,
            pending_mint_program_id: pending_pid,
            txo_buffer_program_id: txo_pid,
            generic_buffer_program_id: generic_pid,
            doge_mint: doge_mint.pubkey(),
        };

        Self {
            context,
            program_id: bridge_pid,
            manual_claim_pid: manual_pid,
            pending_mint_pid: pending_pid,
            txo_buffer_pid: txo_pid,
            generic_buffer_pid: generic_pid,
            doge_mint: doge_mint.pubkey(),
            client: test_client,
        }
    }

    /// Create a mock custodian set manager account for testing.
    /// Returns the account pubkey and the expected custodian config hash.
    pub fn create_mock_custodian_set_manager(&mut self, config_id: u32) -> (Pubkey, [u8; 32]) {
        let (data, expected_hash) = self.client.generate_mock_custodian_config_data(config_id);

        // Create a new account pubkey
        let account_pubkey = Pubkey::new_unique();

        // Create account with the mock data
        let account = Account {
            lamports: 1_000_000, // 0.001 SOL should be enough
            data,
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: 0,
        };

        // Set the account in the context
        self.context.set_account(&account_pubkey, &account.into());

        (account_pubkey, expected_hash)
    }
}