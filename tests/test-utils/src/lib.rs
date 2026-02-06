pub mod mock_data;
pub mod test_client;
pub mod block_transition_helper;
pub mod builders;
pub mod tree;
pub mod constants;

use borsh::BorshSerialize;
use doge_bridge::processor::process_instruction as doge_bridge_processor;
use delegated_manager_set::process_instruction as delegated_manager_set_processor;
use delegated_manager_set_types::{
    ManagerSet, ManagerSetIndex, DOGECOIN_CHAIN_ID, MANAGER_SET_DATA_SIZE, MANAGER_SET_DISC,
    MANAGER_SET_INDEX_DISC, MANAGER_SET_PREFIX, PROGRAM_ID as DELEGATED_MANAGER_SET_PROGRAM_ID,
};
use generic_buffer::process_instruction as generic_buffer_processor;
use manual_claim::processor::process_instruction as manual_claim_processor;
use pending_mint_buffer::process_instruction as pending_mint_processor;
use psy_bridge_core::custodian_config::FullMultisigCustodianConfig;
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
    pub delegated_manager_set_pid: Pubkey,
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
        let delegated_manager_set_pid = DELEGATED_MANAGER_SET_PROGRAM_ID;

        let mut pt = ProgramTest::default();

        // Add all programs to the test runner
        pt.add_program("doge_bridge", bridge_pid, processor!(doge_bridge_processor));
        pt.add_program("manual_claim", manual_pid, processor!(manual_claim_processor));
        pt.add_program("pending_mint_buffer", pending_pid, processor!(pending_mint_processor));
        pt.add_program("txo_buffer", txo_pid, processor!(txo_buffer_processor));
        pt.add_program("generic_buffer", generic_pid, processor!(generic_buffer_processor));
        pt.add_program("delegated_manager_set", delegated_manager_set_pid, processor!(delegated_manager_set_processor));

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
            delegated_manager_set_pid,
            doge_mint: doge_mint.pubkey(),
            client: test_client,
        }
    }

    /// Create mock delegated manager set accounts for testing (ManagerSetIndex + ManagerSet).
    /// Returns (manager_set_index_pda, manager_set_pda, expected_custodian_config_hash).
    ///
    /// The manager_set data contains:
    /// - 3 bytes prefix: 01 05 07
    /// - 7 SEC1 compressed secp256k1 public keys (33 bytes each)
    pub fn create_mock_manager_set(&mut self, config_index: u32) -> (Pubkey, Pubkey, [u8; 32]) {
        // Generate 7 test compressed public keys
        // Each compressed key is 33 bytes: [prefix(02 or 03)][x-coordinate(32 bytes)]
        let mut compressed_keys: [[u8; 33]; 7] = [[0u8; 33]; 7];
        for (i, key) in compressed_keys.iter_mut().enumerate() {
            key[0] = 0x02; // Even y-parity prefix
            // Fill x-coordinate with test data (i+1 repeated)
            key[1..].fill((i + 1) as u8);
        }

        // Build manager_set data: 3-byte prefix + 7 * 33-byte compressed keys
        let mut manager_set_data = Vec::with_capacity(MANAGER_SET_DATA_SIZE);
        manager_set_data.extend_from_slice(&MANAGER_SET_PREFIX);
        for key in &compressed_keys {
            manager_set_data.extend_from_slice(key);
        }

        // Create ManagerSetIndex account
        let (msi_pda, _) = ManagerSetIndex::pda(DOGECOIN_CHAIN_ID);
        let msi = ManagerSetIndex {
            manager_chain_id: DOGECOIN_CHAIN_ID,
            current_index: config_index,
        };
        let mut msi_data = vec![0u8; ManagerSetIndex::SIZE];
        msi_data[..8].copy_from_slice(&MANAGER_SET_INDEX_DISC);
        let msi_encoded = msi.try_to_vec().unwrap();
        msi_data[8..8 + msi_encoded.len()].copy_from_slice(&msi_encoded);

        let msi_account = Account {
            lamports: 1_000_000,
            data: msi_data,
            owner: DELEGATED_MANAGER_SET_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        };
        self.context.set_account(&msi_pda, &msi_account.into());

        // Create ManagerSet account
        let (ms_pda, _) = ManagerSet::pda(DOGECOIN_CHAIN_ID, config_index);
        let ms = ManagerSet {
            manager_chain_id: DOGECOIN_CHAIN_ID,
            index: config_index,
            manager_set: manager_set_data.clone(),
        };
        let ms_size = ManagerSet::size(manager_set_data.len());
        let mut ms_data = vec![0u8; ms_size];
        ms_data[..8].copy_from_slice(&MANAGER_SET_DISC);
        let ms_encoded = ms.try_to_vec().unwrap();
        ms_data[8..8 + ms_encoded.len()].copy_from_slice(&ms_encoded);

        let ms_account = Account {
            lamports: 1_000_000,
            data: ms_data,
            owner: DELEGATED_MANAGER_SET_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        };
        self.context.set_account(&ms_pda, &ms_account.into());

        // Compute the expected custodian config hash
        // Skip the 3-byte prefix to get the compressed keys
        let compressed_keys_buf = &manager_set_data[3..];
        let (bridge_pda, _) = Pubkey::find_program_address(&[b"bridge_state"], &self.program_id);
        let full_config = FullMultisigCustodianConfig::from_compressed_public_keys_buf(
            bridge_pda.to_bytes(),
            compressed_keys_buf,
            config_index,
            0, // network_type
        ).expect("Failed to create FullMultisigCustodianConfig");
        let expected_hash = full_config.get_wallet_config_hash();

        (msi_pda, ms_pda, expected_hash)
    }

    /// Create mock manager set with custom compressed public keys.
    /// Returns (manager_set_index_pda, manager_set_pda, expected_custodian_config_hash).
    pub fn create_mock_manager_set_with_keys(
        &mut self,
        config_index: u32,
        compressed_keys: &[[u8; 33]; 7],
    ) -> (Pubkey, Pubkey, [u8; 32]) {
        // Build manager_set data: 3-byte prefix + 7 * 33-byte compressed keys
        let mut manager_set_data = Vec::with_capacity(MANAGER_SET_DATA_SIZE);
        manager_set_data.extend_from_slice(&MANAGER_SET_PREFIX);
        for key in compressed_keys {
            manager_set_data.extend_from_slice(key);
        }

        // Create ManagerSetIndex account
        let (msi_pda, _) = ManagerSetIndex::pda(DOGECOIN_CHAIN_ID);
        let msi = ManagerSetIndex {
            manager_chain_id: DOGECOIN_CHAIN_ID,
            current_index: config_index,
        };
        let mut msi_data = vec![0u8; ManagerSetIndex::SIZE];
        msi_data[..8].copy_from_slice(&MANAGER_SET_INDEX_DISC);
        let msi_encoded = msi.try_to_vec().unwrap();
        msi_data[8..8 + msi_encoded.len()].copy_from_slice(&msi_encoded);

        let msi_account = Account {
            lamports: 1_000_000,
            data: msi_data,
            owner: DELEGATED_MANAGER_SET_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        };
        self.context.set_account(&msi_pda, &msi_account.into());

        // Create ManagerSet account
        let (ms_pda, _) = ManagerSet::pda(DOGECOIN_CHAIN_ID, config_index);
        let ms = ManagerSet {
            manager_chain_id: DOGECOIN_CHAIN_ID,
            index: config_index,
            manager_set: manager_set_data.clone(),
        };
        let ms_size = ManagerSet::size(manager_set_data.len());
        let mut ms_data = vec![0u8; ms_size];
        ms_data[..8].copy_from_slice(&MANAGER_SET_DISC);
        let ms_encoded = ms.try_to_vec().unwrap();
        ms_data[8..8 + ms_encoded.len()].copy_from_slice(&ms_encoded);

        let ms_account = Account {
            lamports: 1_000_000,
            data: ms_data,
            owner: DELEGATED_MANAGER_SET_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        };
        self.context.set_account(&ms_pda, &ms_account.into());

        // Compute the expected custodian config hash
        let compressed_keys_buf = &manager_set_data[3..];
        let (bridge_pda, _) = Pubkey::find_program_address(&[b"bridge_state"], &self.program_id);
        let full_config = FullMultisigCustodianConfig::from_compressed_public_keys_buf(
            bridge_pda.to_bytes(),
            compressed_keys_buf,
            config_index,
            0, // network_type
        ).expect("Failed to create FullMultisigCustodianConfig");
        let expected_hash = full_config.get_wallet_config_hash();

        (msi_pda, ms_pda, expected_hash)
    }
}