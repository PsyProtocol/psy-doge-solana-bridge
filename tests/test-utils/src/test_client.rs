use std::collections::HashMap;

use doge_bridge_client::instructions;
use psy_bridge_core::custodian_config::{FullMultisigCustodianConfig, RemoteMultisigCustodianConfig};
use psy_doge_solana_core::data_accounts::pending_mint::{
    PendingMint, PM_DA_PENDING_MINT_SIZE as PENDING_MINT_SIZE, PM_MAX_PENDING_MINTS_PER_GROUP,
};
use solana_program_test::BanksClient;
use solana_sdk::{
    instruction::Instruction,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use spl_token::instruction::{set_authority, AuthorityType};

pub struct UserManager {
    pub user_map: HashMap<Pubkey, Keypair>,
}
impl UserManager {
    pub fn new() -> Self { Self { user_map: HashMap::new() } }
    pub fn get_or_create_user(&mut self, pubkey: &Pubkey) -> Keypair {
        if let Some(user) = self.user_map.get(pubkey) { return Keypair::from_bytes(&user.to_bytes()).unwrap(); }
        let new_user = Keypair::new();
        self.user_map.insert(*pubkey, Keypair::from_bytes(&new_user.to_bytes()).unwrap());
        new_user
    }
}

const CHUNK_SIZE: usize = 900;
pub struct TestBridgeClient {
    pub client: BanksClient,
    pub payer: Keypair,
    pub operator: Keypair,
    pub fee_spender: Keypair,
    pub program_id: Pubkey,
    pub pending_mint_program_id: Pubkey,
    pub txo_buffer_program_id: Pubkey,
    pub generic_buffer_program_id: Pubkey,
    pub bridge_state_pda: Pubkey,
    pub doge_mint: Pubkey,
}
impl Clone for TestBridgeClient {
    fn clone(&self) -> Self {
        Self {
            bridge_state_pda: self.bridge_state_pda,
            client: self.client.clone(),
            payer: Keypair::from_bytes(&self.payer.to_bytes()).unwrap(),
            operator: Keypair::from_bytes(&self.operator.to_bytes()).unwrap(),
            fee_spender: Keypair::from_bytes(&self.fee_spender.to_bytes()).unwrap(),
            program_id: self.program_id,
            pending_mint_program_id: self.pending_mint_program_id,
            txo_buffer_program_id: self.txo_buffer_program_id,
            generic_buffer_program_id: self.generic_buffer_program_id,
            doge_mint: self.doge_mint,
        }
    }
}
impl TestBridgeClient {
    pub async fn send_tx(&self, ixs: &[Instruction], extra_signers: &[&Keypair]) {
        let recent_blockhash = self.client.get_latest_blockhash().await.unwrap();
        let mut signers = vec![&self.payer];
        signers.extend_from_slice(extra_signers);
        let tx = Transaction::new_signed_with_payer(ixs, Some(&self.payer.pubkey()), &signers, recent_blockhash);
        println!("tx_size: {}", tx.message.serialize().len());
        self.client.process_transaction(tx).await.unwrap();
    }
    
    pub async fn create_token_ata_if_needed(&mut self, mint: Pubkey, user_authority: &Keypair) -> u64 {
        let user_token_account = spl_associated_token_account::get_associated_token_address(&user_authority.pubkey(), &mint);
        let account_m = self.client.get_account(user_token_account).await.unwrap();
        match account_m {
            Some(acc) => {
                let token_account_m = spl_token::state::Account::unpack_from_slice(&acc.data).unwrap();
                token_account_m.amount
            }
            None => {
                let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(&self.payer.pubkey(), &user_authority.pubkey(), &mint, &spl_token::id());
                self.send_tx(&[create_ata_ix], &[]).await;
                let null_closer_ix = set_authority(&spl_token::id(), &user_token_account, None, AuthorityType::CloseAccount, &user_authority.pubkey(), &[]).unwrap();
                self.send_tx(&[null_closer_ix], &[user_authority]).await;
                0
            }
        }
    }

    pub async fn create_generic_buffer(&mut self, data: &[u8]) -> Pubkey {
        let buffer_account = Keypair::new();
        let buffer_pubkey = buffer_account.pubkey();
        let target_size = data.len() as u32;
        let space = 32;
        let rent = self.client.get_rent().await.unwrap();
        let min_bal = rent.minimum_balance(space);
        let create_ix = system_instruction::create_account(&self.payer.pubkey(), &buffer_pubkey, min_bal, space as u64, &self.generic_buffer_program_id);
        let init_ix = instructions::generic_buffer_init(self.generic_buffer_program_id, buffer_pubkey, self.payer.pubkey(), target_size);
        self.send_tx(&[create_ix, init_ix], &[&buffer_account]).await;
        for (i, chunk) in data.chunks(CHUNK_SIZE).enumerate() {
            let offset = (i * CHUNK_SIZE) as u32;
            let write_ix = instructions::generic_buffer_write(self.generic_buffer_program_id, buffer_pubkey, self.payer.pubkey(), offset, chunk);
            self.send_tx(&[write_ix], &[]).await;
        }
        buffer_pubkey
    }

    pub async fn create_pending_mint_buffer(&mut self, locker: Pubkey, mints: &[PendingMint]) -> Pubkey {
        let operator_pubkey = self.operator.pubkey().to_bytes();
        let seeds: &[&[u8]] = &[
            b"mint_buffer",
            &operator_pubkey,
        ];
        let (buffer_pubkey, _) = Pubkey::find_program_address(seeds, &self.pending_mint_program_id);

        let account_info = self.client.get_account(buffer_pubkey).await.unwrap();

        if account_info.is_none() {
            let space = 72;
            let rent = self.client.get_rent().await.unwrap();
            let min_bal = rent.minimum_balance(space);
            let transfer_ix = system_instruction::transfer(&self.payer.pubkey(), &buffer_pubkey, min_bal);
            let setup_ix = instructions::pending_mint_setup(self.pending_mint_program_id, buffer_pubkey, locker, self.operator.pubkey());
            self.send_tx(&[transfer_ix, setup_ix], &[]).await;
        }

        let total_mints = mints.len() as u16;
        let reinit_ix = instructions::pending_mint_reinit(self.pending_mint_program_id, buffer_pubkey, self.operator.pubkey(), total_mints);
        self.send_tx(&[reinit_ix], &[&self.operator]).await;

        let groups_count = (mints.len() + PM_MAX_PENDING_MINTS_PER_GROUP - 1) / PM_MAX_PENDING_MINTS_PER_GROUP;
        for group_idx in 0..groups_count {
            let start = group_idx * PM_MAX_PENDING_MINTS_PER_GROUP;
            let end = std::cmp::min(start + PM_MAX_PENDING_MINTS_PER_GROUP, mints.len());
            let group_mints = &mints[start..end];
            let mut mint_data = Vec::with_capacity(group_mints.len() * PENDING_MINT_SIZE);
            for m in group_mints { mint_data.extend_from_slice(bytemuck::bytes_of(m)); }
            let insert_ix = instructions::pending_mint_insert(self.pending_mint_program_id, buffer_pubkey, self.operator.pubkey(), group_idx as u16, &mint_data);
            self.send_tx(&[insert_ix], &[&self.operator]).await;
        }
        println!("total_pending_mints: {}", mints.len());
        let account = self.client.get_account(buffer_pubkey).await.unwrap().unwrap();
        println!("pending_mint_buffe: {}", hex::encode(&account.data[70..(72 + groups_count * 32)]));
        buffer_pubkey
    }

    pub async fn create_txo_buffer(&mut self, doge_block_height: u32, txo_indices: &[u32], batch_id: u32) -> Pubkey {
        let operator_pubkey = self.operator.pubkey().to_bytes();
        let seeds: &[&[u8]] = &[
            b"txo_buffer",
            &operator_pubkey,
        ];
        let (buffer_pubkey, _) = Pubkey::find_program_address(seeds, &self.txo_buffer_program_id);

        let account_info = self.client.get_account(buffer_pubkey).await.unwrap();

        if account_info.is_none() {
            let space = 48;
            let rent = self.client.get_rent().await.unwrap();
            let min_bal = rent.minimum_balance(space);
            let transfer_ix = system_instruction::transfer(&self.payer.pubkey(), &buffer_pubkey, min_bal);
            let init_ix = instructions::txo_buffer_init(self.txo_buffer_program_id, buffer_pubkey, self.operator.pubkey());
            self.send_tx(&[transfer_ix, init_ix], &[]).await;
        }

        let txo_bytes: Vec<u8> = txo_indices.iter().flat_map(|x| x.to_le_bytes()).collect();
        let total_len = txo_bytes.len() as u32;

        let set_len_ix = instructions::txo_buffer_set_len(self.txo_buffer_program_id, buffer_pubkey, self.payer.pubkey(), self.operator.pubkey(), total_len, true, batch_id, doge_block_height, false);
        self.send_tx(&[set_len_ix], &[&self.operator]).await;

        for (i, chunk) in txo_bytes.chunks(CHUNK_SIZE).enumerate() {
            let offset = (i * CHUNK_SIZE) as u32;
            let write_ix = instructions::txo_buffer_write(self.txo_buffer_program_id, buffer_pubkey, self.operator.pubkey(), batch_id, offset, chunk);
            self.send_tx(&[write_ix], &[&self.operator]).await;
        }

        let finalize_ix = instructions::txo_buffer_set_len(self.txo_buffer_program_id, buffer_pubkey, self.payer.pubkey(), self.operator.pubkey(), total_len, false, batch_id, doge_block_height, true);
        self.send_tx(&[finalize_ix], &[&self.operator]).await;
        buffer_pubkey
    }

    /// Get mint buffer PDA (derived from operator key)
    pub fn get_mint_buffer_pda(&self) -> Pubkey {
        let operator_pubkey = self.operator.pubkey().to_bytes();
        let (buffer_pubkey, _) = Pubkey::find_program_address(
            &[b"mint_buffer", &operator_pubkey],
            &self.pending_mint_program_id,
        );
        buffer_pubkey
    }

    /// Get TXO buffer PDA (derived from operator key)
    pub fn get_txo_buffer_pda(&self) -> Pubkey {
        let operator_pubkey = self.operator.pubkey().to_bytes();
        let (buffer_pubkey, _) = Pubkey::find_program_address(
            &[b"txo_buffer", &operator_pubkey],
            &self.txo_buffer_program_id,
        );
        buffer_pubkey
    }

    // ========================================================================
    // Custodian Transition Helpers
    // ========================================================================

    /// Notify the bridge of a custodian config update (starts grace period)
    pub async fn notify_custodian_config_update(
        &mut self,
        custodian_set_manager_account: Pubkey,
        expected_new_custodian_config_hash: [u8; 32],
    ) {
        let ix = instructions::notify_custodian_config_update(
            self.program_id,
            self.operator.pubkey(),
            custodian_set_manager_account,
            expected_new_custodian_config_hash,
        );
        self.send_tx(&[ix], &[&self.operator]).await;
    }

    /// Pause deposits for custodian transition (after grace period)
    pub async fn pause_deposits_for_transition(&mut self) {
        let ix = instructions::pause_deposits_for_transition(
            self.program_id,
            self.operator.pubkey(),
        );
        self.send_tx(&[ix], &[&self.operator]).await;
    }

    /// Cancel a pending custodian transition
    pub async fn cancel_custodian_transition(&mut self) {
        let ix = instructions::cancel_custodian_transition(
            self.program_id,
            self.operator.pubkey(),
        );
        self.send_tx(&[ix], &[&self.operator]).await;
    }

    /// Generate mock custodian set manager account data and the expected config hash.
    /// Returns (account_data, expected_hash) that can be used with context.set_account().
    pub fn generate_mock_custodian_config_data(&self, config_id: u32) -> (Vec<u8>, [u8; 32]) {
        // Create a mock RemoteMultisigCustodianConfig with test data
        let remote_config = RemoteMultisigCustodianConfig {
            signer_public_keys: [
                [1u8; 32], [2u8; 32], [3u8; 32], [4u8; 32],
                [5u8; 32], [6u8; 32], [7u8; 32],
            ],
            custodian_config_id: config_id,
            signer_public_keys_y_parity: 0,
        };

        // Account layout: 8 bytes discriminator + 32 bytes operator + RemoteMultisigCustodianConfig
        let config_size = std::mem::size_of::<RemoteMultisigCustodianConfig>();
        let total_size = 8 + 32 + config_size;
        let mut data = vec![0u8; total_size];

        // Set discriminator (arbitrary test value)
        data[0..8].copy_from_slice(&[0xCC; 8]);

        // Set operator pubkey
        data[8..40].copy_from_slice(&self.operator.pubkey().to_bytes());

        // Set RemoteMultisigCustodianConfig
        data[40..].copy_from_slice(bytemuck::bytes_of(&remote_config));

        // Compute the expected hash using FullMultisigCustodianConfig
        let full_config = FullMultisigCustodianConfig {
            emitter_pubkey: self.bridge_state_pda.to_bytes(),
            signer_public_keys: remote_config.signer_public_keys,
            custodian_config_id: remote_config.custodian_config_id,
            signer_public_keys_y_parity: remote_config.signer_public_keys_y_parity as u16,
            network_type: 0,
        };
        let expected_hash = full_config.get_wallet_config_hash();

        (data, expected_hash)
    }
}
