//! End-to-end example using the BridgeClient from clients/rust
//!
//! This example demonstrates how to:
//! 1. Initialize the bridge
//! 2. Process block transitions with deposits
//! 3. Handle minting to users
//! 4. Query bridge state
//! 5. Request withdrawals (burn tokens)
//!
//! Prerequisites:
//! - Local Solana validator running (solana-test-validator)
//! - Programs deployed (make deploy-programs)
//!
//! Run with:
//! ```bash
//! cargo run --example e2e_bridge_client
//! ```

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use doge_bridge_client::{
    BridgeApi, BridgeClient, BridgeClientConfigBuilder, InitializeBridgeParams, OperatorApi,
    PendingMint, PsyBridgeConfig, PsyBridgeHeader, PsyBridgeStateCommitment,
    PsyBridgeTipStateCommitment, PsyReturnTxOutput,
    WithdrawalApi,
};
use psy_bridge_core::crypto::hash::sha256_impl::hash_impl_sha256_bytes;
use psy_doge_solana_core::{
    data_accounts::pending_mint::{PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH, PM_TXO_DEFAULT_BUFFER_HASH},
    public_inputs::get_block_transition_public_inputs,
};
use doge_bridge_test_utils::mock_data::generate_block_update_fake_proof;
use doge_bridge_test_utils::builders::pending_mints_buffer_builder::PendingMintsGroupsBufferBuilder;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use spl_associated_token_account::get_associated_token_address;


/// Configuration for the example
struct ExampleConfig {
    rpc_url: String,
}

impl Default for ExampleConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://127.0.0.1:8899".to_string(),
        }
    }
}

/// Represents a deposit for testing
#[derive(Debug, Clone)]
pub struct Deposit {
    pub recipient: Pubkey,
    pub amount_sats: u64,
    pub txo_index: u32,
}

impl Deposit {
    pub fn new(recipient: Pubkey, amount_sats: u64, txo_index: u32) -> Self {
        Self {
            recipient,
            amount_sats,
            txo_index,
        }
    }
}

/// Helper to manage the end-to-end example state
struct E2EContext {
    rpc: RpcClient,
    bridge_client: BridgeClient,
    doge_mint: Pubkey,
    payer: Keypair,
    users: HashMap<Pubkey, Keypair>,
}

impl E2EContext {
    /// Create a new E2E context
    async fn new(config: ExampleConfig) -> Result<Self> {
        let rpc = RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        );

        // Verify connection
        rpc.get_health().await
            .map_err(|e| anyhow!("Failed to connect to RPC: {}. Is the validator running?", e))?;

        println!("Connected to RPC at {}", config.rpc_url);

        // Create keypairs
        let payer = Keypair::new();
        let operator = Keypair::from_bytes(&payer.to_bytes())?;
        let doge_mint_keypair = Keypair::new();
        let doge_mint = doge_mint_keypair.pubkey();

        // Airdrop SOL to payer
        println!("Airdropping SOL to payer...");
        let sig = rpc.request_airdrop(&payer.pubkey(), 100_000_000_000).await?;
        loop {
            let confirmed = rpc.confirm_transaction(&sig).await?;
            if confirmed {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        println!("Payer balance: {} SOL", rpc.get_balance(&payer.pubkey()).await? as f64 / 1e9);

        // Derive bridge state PDA
        let bridge_state_pda = Self::derive_bridge_state_pda();

        // Create the DOGE mint
        println!("Creating DOGE mint...");
        Self::create_mint(&rpc, &payer, &doge_mint_keypair, &bridge_state_pda).await?;
        println!("DOGE mint created: {}", doge_mint);

        // Create bridge client
        let wormhole_core = Pubkey::new_unique();
        let wormhole_shim = Pubkey::new_unique();

        let client_config = BridgeClientConfigBuilder::new()
            .rpc_url(&config.rpc_url)
            .operator(operator)
            .payer(Keypair::from_bytes(&payer.to_bytes())?)
            .bridge_state_pda(bridge_state_pda)
            .wormhole_core_program_id(wormhole_core)
            .wormhole_shim_program_id(wormhole_shim)
            .doge_mint(doge_mint)
            .build()?;

        let bridge_client = BridgeClient::with_config(client_config)?;

        Ok(Self {
            rpc,
            bridge_client,
            doge_mint,
            payer,
            users: HashMap::new(),
        })
    }

    /// Derive the bridge state PDA
    fn derive_bridge_state_pda() -> Pubkey {
        // This should match the program's PDA derivation
        let program_id = Self::get_bridge_program_id();
        let (pda, _bump) = Pubkey::find_program_address(&[b"bridge_state"], &program_id);
        pda
    }

    /// Get the bridge program ID (from keypair file)
    fn get_bridge_program_id() -> Pubkey {
        // In a real scenario, load from program-keys/doge-bridge.json
        // For the example, we use the default test program ID
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let keypair_path = std::path::Path::new(manifest_dir)
            .parent()
            .unwrap()
            .join("tests/local-network-tests/program-keys/doge-bridge.json");

        if keypair_path.exists() {
            let data = std::fs::read_to_string(&keypair_path)
                .expect("Failed to read doge-bridge keypair");
            let bytes: Vec<u8> = serde_json::from_str(&data)
                .expect("Failed to parse keypair JSON");
            Keypair::from_bytes(&bytes)
                .expect("Invalid keypair")
                .pubkey()
        } else {
            // Fallback for testing
            Pubkey::new_unique()
        }
    }

    /// Create the DOGE mint token
    async fn create_mint(
        rpc: &RpcClient,
        payer: &Keypair,
        mint_keypair: &Keypair,
        mint_authority: &Pubkey,
    ) -> Result<()> {
        let rent = rpc.get_minimum_balance_for_rent_exemption(
            spl_token::state::Mint::LEN
        ).await?;

        let create_ix = system_instruction::create_account(
            &payer.pubkey(),
            &mint_keypair.pubkey(),
            rent,
            spl_token::state::Mint::LEN as u64,
            &spl_token::id(),
        );

        let init_ix = spl_token::instruction::initialize_mint(
            &spl_token::id(),
            &mint_keypair.pubkey(),
            mint_authority,
            None,
            8, // decimals
        )?;

        let blockhash = rpc.get_latest_blockhash().await?;
        let tx = Transaction::new_signed_with_payer(
            &[create_ix, init_ix],
            Some(&payer.pubkey()),
            &[payer, mint_keypair],
            blockhash,
        );

        rpc.send_and_confirm_transaction(&tx).await?;
        Ok(())
    }

    /// Add a new user and return their pubkey
    fn add_user(&mut self) -> Pubkey {
        let user = Keypair::new();
        let pubkey = user.pubkey();
        self.users.insert(pubkey, user);
        pubkey
    }

    /// Create ATA for a user if needed
    async fn create_user_ata(&self, user: &Pubkey) -> Result<Pubkey> {
        let ata = get_associated_token_address(user, &self.doge_mint);

        // Check if ATA exists
        if self.rpc.get_account(&ata).await.is_ok() {
            return Ok(ata);
        }

        let create_ata_ix = spl_associated_token_account::instruction::create_associated_token_account(
            &self.payer.pubkey(),
            user,
            &self.doge_mint,
            &spl_token::id(),
        );

        let blockhash = self.rpc.get_latest_blockhash().await?;
        let tx = Transaction::new_signed_with_payer(
            &[create_ata_ix],
            Some(&self.payer.pubkey()),
            &[&self.payer],
            blockhash,
        );

        self.rpc.send_and_confirm_transaction(&tx).await?;
        Ok(ata)
    }

    /// Get token balance for a user
    async fn get_user_balance(&self, user: &Pubkey) -> Result<u64> {
        let ata = get_associated_token_address(user, &self.doge_mint);

        match self.rpc.get_token_account_balance(&ata).await {
            Ok(balance) => {
                let amount: u64 = balance.amount.parse()?;
                Ok(amount)
            }
            Err(_) => Ok(0),
        }
    }

    /// Initialize the bridge
    async fn initialize_bridge(&self) -> Result<()> {
        println!("\nInitializing bridge...");

        let params = InitializeBridgeParams {
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
            config_params: PsyBridgeConfig {
                deposit_fee_rate_numerator: 0,
                deposit_fee_rate_denominator: 100,
                withdrawal_fee_rate_numerator: 0,
                withdrawal_fee_rate_denominator: 100,
                deposit_flat_fee_sats: 0,
                withdrawal_flat_fee_sats: 0,
            },
        };

        let sig = self.bridge_client.initialize_bridge(&params).await?;
        println!("Bridge initialized! Signature: {}", sig);

        Ok(())
    }

    /// Prepare pending mints and compute hashes
    async fn prepare_block_data(
        &self,
        deposits: &[Deposit],
    ) -> Result<(Vec<PendingMint>, [u8; 32], [u8; 32])> {
        let mut pending_mints = Vec::with_capacity(deposits.len());

        for d in deposits {
            // Get the user's ATA
            let ata = get_associated_token_address(&d.recipient, &self.doge_mint);

            pending_mints.push(PendingMint {
                recipient: ata.to_bytes(),
                amount: d.amount_sats,
            });
        }

        // Compute TXO hash
        let txo_indices: Vec<u32> = deposits.iter().map(|d| d.txo_index).collect();
        let txo_bytes: Vec<u8> = txo_indices.iter().flat_map(|x| x.to_le_bytes()).collect();
        let txo_hash = if txo_bytes.is_empty() {
            PM_TXO_DEFAULT_BUFFER_HASH
        } else {
            hash_impl_sha256_bytes(&txo_bytes)
        };

        // Compute pending mints hash
        let pending_mints_hash = if pending_mints.is_empty() {
            PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH
        } else {
            let mut builder = PendingMintsGroupsBufferBuilder::new_with_hint(pending_mints.len());
            for pm in &pending_mints {
                builder.append_pending_mint(&pm.recipient, pm.amount);
            }
            builder.finalize()?.finalized_hash
        };

        Ok((pending_mints, pending_mints_hash, txo_hash))
    }

    /// Process a block with deposits using BridgeClient
    async fn process_block(&mut self, deposits: Vec<Deposit>) -> Result<()> {
        // Create ATAs for all recipients first
        for d in &deposits {
            self.create_user_ata(&d.recipient).await?;
        }

        // Get current state
        let state = self.bridge_client.get_current_bridge_state().await?;

        // Prepare block data
        let (pending_mints, pending_mints_hash, txo_hash) = self
            .prepare_block_data(&deposits)
            .await?;

        let txo_indices: Vec<u32> = deposits.iter().map(|d| d.txo_index).collect();

        // Build new header
        let mut new_header = state.bridge_header.clone();
        new_header.finalized_state.block_height += 1;
        new_header.finalized_state.pending_mints_finalized_hash = pending_mints_hash;
        new_header.finalized_state.txo_output_list_finalized_hash = txo_hash;
        new_header.finalized_state.auto_claimed_deposits_next_index += pending_mints.len() as u32;
        new_header.tip_state = PsyBridgeTipStateCommitment {
            block_hash: [1u8; 32],
            block_merkle_tree_root: [1u8; 32],
            block_time: new_header.tip_state.block_time + 60,
            block_height: new_header.tip_state.block_height + 1,
        };

        let new_height = new_header.finalized_state.block_height;

        // Generate fake proof
        let pub_inputs = get_block_transition_public_inputs(
            &state.bridge_header.get_hash_canonical(),
            &new_header.get_hash_canonical(),
            &state.config_params.get_hash(),
            &state.custodian_wallet_config_hash,
        );
        let proof = generate_block_update_fake_proof(pub_inputs);

        println!(
            "\nProcessing Block {}: {} deposits",
            new_height,
            deposits.len()
        );

        // Setup buffers using BridgeClient
        let (mint_buffer, mint_bump) = self.bridge_client
            .setup_pending_mints_buffer(new_height, &pending_mints)
            .await?;

        let (txo_buffer, txo_bump) = self.bridge_client
            .setup_txo_buffer(new_height, &txo_indices)
            .await?;

        println!("Buffers created:");
        println!("  Mint buffer: {}", mint_buffer);
        println!("  TXO buffer: {}", txo_buffer);

        // Process block transition
        let sig = self.bridge_client
            .process_block_transition(
                proof,
                new_header,
                mint_buffer,
                mint_bump,
                txo_buffer,
                txo_bump,
            )
            .await?;

        println!("Block transition sent! Signature: {}", sig);

        // Process pending mints
        if !pending_mints.is_empty() {
            println!("Processing {} pending mints...", pending_mints.len());

            let result = self.bridge_client
                .process_remaining_pending_mints_groups(
                    &pending_mints,
                    mint_buffer,
                    mint_bump,
                )
                .await?;

            println!(
                "Mints processed: {} groups, {} total mints",
                result.groups_processed,
                result.total_mints_processed
            );
        }

        Ok(())
    }

    /// Query and print current bridge state
    async fn print_bridge_state(&self) -> Result<()> {
        let state = self.bridge_client.get_current_bridge_state().await?;

        println!("\n=== Bridge State ===");
        println!(
            "Block height: {}",
            state.bridge_header.finalized_state.block_height
        );
        println!(
            "Auto-claimed deposits: {}",
            state.bridge_header.finalized_state.auto_claimed_deposits_next_index
        );
        println!(
            "Tip block height: {}",
            state.bridge_header.tip_state.block_height
        );
        println!("====================");

        Ok(())
    }

    /// Get a user's keypair by pubkey
    fn get_user_keypair(&self, user: &Pubkey) -> Option<Keypair> {
        self.users.get(user).map(|k| Keypair::from_bytes(&k.to_bytes()).unwrap())
    }

    /// Request a withdrawal (burn tokens to get DOGE on Dogecoin network)
    async fn request_withdrawal(
        &self,
        user: &Pubkey,
        amount_sats: u64,
        recipient_doge_address: [u8; 20],
    ) -> Result<()> {
        let user_keypair = self.get_user_keypair(user)
            .ok_or_else(|| anyhow!("User {} not found", user))?;

        println!(
            "\nRequesting withdrawal: {} sats from {}",
            amount_sats, user
        );

        // address_type: 0 = P2PKH (legacy), 1 = P2SH
        let address_type = 0u32;

        let sig = self.bridge_client
            .request_withdrawal(&user_keypair, recipient_doge_address, amount_sats, address_type)
            .await?;

        println!("Withdrawal requested! Signature: {}", sig);

        Ok(())
    }

    /// Print withdrawal snapshot details
    async fn print_withdrawal_snapshot(&self) -> Result<()> {
        let snapshot = self.bridge_client.snapshot_withdrawals().await?;

        println!("\n=== Withdrawal Snapshot ===");
        println!(
            "Next withdrawal index: {}",
            snapshot.next_requested_withdrawals_tree_index
        );
        println!(
            "Block height: {}",
            snapshot.block_height
        );
        println!("===========================");

        Ok(())
    }

    /// Execute the snapshot withdrawals instruction on-chain
    async fn execute_snapshot_withdrawals(&self) -> Result<()> {
        println!("\nExecuting snapshot withdrawals instruction...");

        let sig = self.bridge_client.execute_snapshot_withdrawals().await?;
        println!("Snapshot withdrawals executed! Signature: {}", sig);

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Bypass proxy for localhost connections
    std::env::set_var("no_proxy", "localhost,127.0.0.1");
    std::env::set_var("NO_PROXY", "localhost,127.0.0.1");

    println!("=== Doge Bridge E2E Example using BridgeClient ===\n");

    // Create context
    let config = ExampleConfig::default();
    let mut ctx = E2EContext::new(config).await?;

    // Initialize bridge
    ctx.initialize_bridge().await?;

    // Print initial state
    ctx.print_bridge_state().await?;

    // Add users
    let user1 = ctx.add_user();
    let user2 = ctx.add_user();
    let user3 = ctx.add_user();

    println!("\nCreated users:");
    println!("  User 1: {}", user1);
    println!("  User 2: {}", user2);
    println!("  User 3: {}", user3);

    // Process Block 1: Single deposit
    let deposits_block1 = vec![
        Deposit::new(user1, 100_000_000, 1), // 1 DOGE
    ];
    ctx.process_block(deposits_block1).await?;

    // Verify user1 balance
    let balance1 = ctx.get_user_balance(&user1).await?;
    println!("\nUser 1 balance after Block 1: {} sats", balance1);
    assert_eq!(balance1, 100_000_000, "User 1 balance mismatch");

    // Process Block 2: Multiple deposits
    let deposits_block2 = vec![
        Deposit::new(user1, 50_000_000, 2),  // Additional to user1
        Deposit::new(user2, 200_000_000, 3), // 2 DOGE to user2
        Deposit::new(user3, 75_000_000, 4),  // 0.75 DOGE to user3
    ];
    ctx.process_block(deposits_block2).await?;

    // Verify all balances
    let balance1 = ctx.get_user_balance(&user1).await?;
    let balance2 = ctx.get_user_balance(&user2).await?;
    let balance3 = ctx.get_user_balance(&user3).await?;

    println!("\nBalances after Block 2:");
    println!("  User 1: {} sats (expected: 150,000,000)", balance1);
    println!("  User 2: {} sats (expected: 200,000,000)", balance2);
    println!("  User 3: {} sats (expected: 75,000,000)", balance3);

    assert_eq!(balance1, 150_000_000, "User 1 balance mismatch");
    assert_eq!(balance2, 200_000_000, "User 2 balance mismatch");
    assert_eq!(balance3, 75_000_000, "User 3 balance mismatch");

    // Process Block 3: Many deposits (tests multiple mint groups)
    let mut deposits_block3 = Vec::new();
    for i in 0..30 {
        let user = ctx.add_user();
        deposits_block3.push(Deposit::new(user, 10_000_000, 100 + i));
    }
    ctx.process_block(deposits_block3).await?;

    // Print state after deposits
    ctx.print_bridge_state().await?;

    // =========================================================================
    // Withdrawal Tests
    // =========================================================================
    println!("\n--- Testing Withdrawals ---");

    // Check initial withdrawal snapshot
    ctx.print_withdrawal_snapshot().await?;

    // User 1 requests a withdrawal (burns 25,000,000 sats = 0.25 DOGE)
    // Fake Dogecoin address (20 bytes for P2PKH hash160)
    let doge_recipient_1: [u8; 20] = [
        0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
        0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
    ];

    let balance_before = ctx.get_user_balance(&user1).await?;
    println!("\nUser 1 balance before withdrawal: {} sats", balance_before);

    ctx.request_withdrawal(&user1, 25_000_000, doge_recipient_1).await?;

    let balance_after = ctx.get_user_balance(&user1).await?;
    println!("User 1 balance after withdrawal: {} sats", balance_after);
    assert_eq!(
        balance_after,
        balance_before - 25_000_000,
        "Balance should decrease by withdrawal amount"
    );

    // Check withdrawal snapshot updated
    ctx.print_withdrawal_snapshot().await?;

    // User 2 requests a larger withdrawal
    let doge_recipient_2: [u8; 20] = [
        0xCA, 0xFE, 0xBA, 0xBE, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00,
    ];

    let balance2_before = ctx.get_user_balance(&user2).await?;
    println!("\nUser 2 balance before withdrawal: {} sats", balance2_before);

    ctx.request_withdrawal(&user2, 100_000_000, doge_recipient_2).await?;

    let balance2_after = ctx.get_user_balance(&user2).await?;
    println!("User 2 balance after withdrawal: {} sats", balance2_after);
    assert_eq!(
        balance2_after,
        balance2_before - 100_000_000,
        "Balance should decrease by withdrawal amount"
    );

    // Final withdrawal snapshot
    ctx.print_withdrawal_snapshot().await?;

    // =========================================================================
    // Snapshot Withdrawals Tests
    // =========================================================================
    println!("\n--- Testing Snapshot Withdrawals ---");

    // Get state before snapshot
    let snapshot_before = ctx.bridge_client.snapshot_withdrawals().await?;
    println!(
        "Before snapshot - withdrawal index: {}, block height: {}",
        snapshot_before.next_requested_withdrawals_tree_index,
        snapshot_before.block_height
    );

    // Execute the snapshot withdrawals instruction
    ctx.execute_snapshot_withdrawals().await?;

    // Verify snapshot was updated
    let snapshot_after = ctx.bridge_client.snapshot_withdrawals().await?;
    println!(
        "After snapshot - withdrawal index: {}, block height: {}",
        snapshot_after.next_requested_withdrawals_tree_index,
        snapshot_after.block_height
    );

    // The snapshot should capture the current withdrawal chain state
    // After two withdrawal requests, the index should be >= 2
    assert!(
        snapshot_after.next_requested_withdrawals_tree_index >= 2,
        "Snapshot should capture at least 2 pending withdrawals"
    );

    println!("Snapshot withdrawals test passed!");

    // Print final bridge state
    ctx.print_bridge_state().await?;

    // Final balance verification
    println!("\n--- Final Balances ---");
    let final_balance1 = ctx.get_user_balance(&user1).await?;
    let final_balance2 = ctx.get_user_balance(&user2).await?;
    let final_balance3 = ctx.get_user_balance(&user3).await?;
    println!("User 1: {} sats (started 150M, withdrew 25M)", final_balance1);
    println!("User 2: {} sats (started 200M, withdrew 100M)", final_balance2);
    println!("User 3: {} sats (no withdrawals)", final_balance3);

    assert_eq!(final_balance1, 125_000_000, "User 1 final balance mismatch");
    assert_eq!(final_balance2, 100_000_000, "User 2 final balance mismatch");
    assert_eq!(final_balance3, 75_000_000, "User 3 final balance mismatch");

    println!("\n=== E2E Example Complete ===");
    println!("All tests passed!");

    Ok(())
}
