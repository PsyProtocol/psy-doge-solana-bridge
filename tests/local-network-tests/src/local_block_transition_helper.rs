use std::collections::HashMap;

use anyhow::Result;
use doge_bridge::state::BridgeState;
use psy_bridge_core::{
    crypto::hash::sha256_impl::hash_impl_sha256_bytes,
    header::PsyBridgeTipStateCommitment,
};
use psy_doge_solana_core::{
    data_accounts::pending_mint::{PendingMint, PM_MAX_PENDING_MINTS_PER_GROUP, PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH, PM_TXO_DEFAULT_BUFFER_HASH},
    program_state::FinalizedBlockMintTxoInfo,
    public_inputs::{get_block_transition_public_inputs, get_reorg_block_transition_public_inputs},
};
use solana_sdk::{pubkey::Pubkey, signature::{Keypair, Signer}};

use crate::local_test_client::LocalTestClient;
use doge_bridge_test_utils::mock_data::{
    generate_block_update_fake_proof, generate_block_update_reorg_fake_proof,
};
use doge_bridge_test_utils::builders::pending_mints_buffer_builder::PendingMintsGroupsBufferBuilder;

/// Represents an auto-claimed deposit for testing
#[derive(Debug, Clone, PartialEq, Eq, Copy, Hash)]
pub struct BTAutoClaimedDeposit {
    pub depositor_pubkey: [u8; 32],
    pub amount: u64,
    pub txo_index: u32,
}

impl BTAutoClaimedDeposit {
    pub fn new(depositor_pubkey: [u8; 32], amount: u64, txo_index: u32) -> Self {
        Self {
            depositor_pubkey,
            amount,
            txo_index,
        }
    }
}

/// Helper for simulating block transitions on a local Solana network
///
/// Unlike the BanksClient-based version, this reads all state from RPC
/// and doesn't maintain in-memory state tracking.
pub struct LocalBlockTransitionHelper {
    pub client: LocalTestClient,
    pub user_accounts: HashMap<Pubkey, Keypair>,
    current_txo_batch_id: u32,
}

impl LocalBlockTransitionHelper {
    /// Create a new helper from an existing client
    pub async fn new_from_client(client: LocalTestClient) -> Result<Self> {
        Ok(Self {
            client,
            user_accounts: HashMap::new(),
            current_txo_batch_id: 0,
        })
    }

    /// Get a user keypair by pubkey
    pub fn get_user_account(&self, user_pubkey: &Pubkey) -> Option<&Keypair> {
        self.user_accounts.get(user_pubkey)
    }

    /// Add a new random user and return their pubkey
    pub fn add_user(&mut self) -> Pubkey {
        let user = Keypair::new();
        let user_pubkey = user.pubkey();
        self.user_accounts.insert(user_pubkey, user);
        user_pubkey
    }

    /// Read current bridge state from the network
    pub async fn read_bridge_state(&self) -> Result<BridgeState> {
        let data = self.client.get_account_data(&self.client.bridge_state_pda).await?;
        let bridge_state: &BridgeState = bytemuck::from_bytes(&data);
        Ok(bridge_state.clone())
    }

    /// Prepare block data offline (creates ATAs, builds pending mints)
    async fn prepare_block_data_offline(
        &mut self,
        deposits: &[BTAutoClaimedDeposit],
    ) -> Result<(Vec<PendingMint>, [u8; 32], [u8; 32])> {
        let mut pending_mints = Vec::with_capacity(deposits.len());

        for d in deposits {
            let user_pubkey = Pubkey::new_from_array(d.depositor_pubkey);

            // Get or create user keypair
            let user_kp = self.user_accounts.get(&user_pubkey)
                .ok_or_else(|| anyhow::anyhow!("User {} not registered. Call add_user() first.", user_pubkey))?;
            let user_kp_copy = Keypair::from_bytes(&user_kp.to_bytes())?;

            // Create ATA if needed
            self.client.create_token_ata_if_needed(&self.client.doge_mint, &user_kp_copy).await?;

            let user_ata = spl_associated_token_account::get_associated_token_address(
                &user_pubkey,
                &self.client.doge_mint,
            );

            pending_mints.push(PendingMint {
                recipient: user_ata.to_bytes(),
                amount: d.amount,
            });
        }

        // Compute TXO hash
        let txo_indices: Vec<u32> = deposits.iter().map(|d| d.txo_index).collect();
        let txo_bytes: Vec<u8> = txo_indices.iter().flat_map(|x| x.to_le_bytes()).collect();
        let txo_hash = if txo_bytes.is_empty() {
            // Default empty hash: sha256([])
            PM_TXO_DEFAULT_BUFFER_HASH
        } else {
            hash_impl_sha256_bytes(&txo_bytes)
        };

        // Compute pending mints hash
        let pending_mints_hash = if pending_mints.is_empty() {
            // Default empty hash: sha256([0u8; 2])
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

    /// Mine and process a single block with the given deposits
    pub async fn mine_and_process_block(
        &mut self,
        auto_claimed_deposits: Vec<BTAutoClaimedDeposit>,
    ) -> Result<()> {
        // Read current state from network
        let bridge_state = self.read_bridge_state().await?;

        // Prepare block data
        let (pending_mints, pending_mints_hash, txo_buffer_hash) = self
            .prepare_block_data_offline(&auto_claimed_deposits)
            .await?;

        let txo_indices: Vec<u32> = auto_claimed_deposits.iter().map(|d| d.txo_index).collect();

        // Build new header
        let mut new_header = bridge_state.core_state.bridge_header.clone();
        new_header.finalized_state.block_height += 1;
        new_header.finalized_state.pending_mints_finalized_hash = pending_mints_hash;
        new_header.finalized_state.txo_output_list_finalized_hash = txo_buffer_hash;
        new_header.finalized_state.auto_claimed_deposits_next_index += pending_mints.len() as u32;
        new_header.tip_state = PsyBridgeTipStateCommitment {
            block_hash: [1u8; 32],
            block_merkle_tree_root: [1u8; 32],
            block_time: new_header.tip_state.block_time + 60,
            block_height: new_header.tip_state.block_height + 1,
        };

        // Generate fake proof
        let pub_inputs = get_block_transition_public_inputs(
            &bridge_state.core_state.bridge_header.get_hash_canonical(),
            &new_header.get_hash_canonical(),
            &bridge_state.core_state.config_params.get_hash(),
            &bridge_state.core_state.custodian_wallet_config_hash,
        );
        let proof = generate_block_update_fake_proof(pub_inputs);

        let new_height = new_header.finalized_state.block_height;

        // Create buffers
        let (mint_buffer, mint_bump) = self.client
            .create_pending_mint_buffer(self.client.bridge_state_pda, &pending_mints)
            .await?;

        self.current_txo_batch_id += 1;
        let (txo_buffer, txo_bump) = self.client
            .create_txo_buffer(new_height, &txo_indices)
            .await?;

        println!(
            "Mining Block {}: {} Deposits",
            new_height,
            pending_mints.len()
        );

        // Send block update
        self.client.send_block_update(
            proof,
            new_header,
            mint_buffer,
            txo_buffer,
            mint_bump,
            txo_bump,
        ).await?;

        println!("Block update sent successfully");

        // Process mint groups
        if !pending_mints.is_empty() {
            let groups_count = (pending_mints.len() + PM_MAX_PENDING_MINTS_PER_GROUP - 1)
                / PM_MAX_PENDING_MINTS_PER_GROUP;

            for i in 0..groups_count {
                let start = i * PM_MAX_PENDING_MINTS_PER_GROUP;
                let end = std::cmp::min(start + PM_MAX_PENDING_MINTS_PER_GROUP, pending_mints.len());
                let group_mints = &pending_mints[start..end];

                let recipient_accounts: Vec<Pubkey> = group_mints
                    .iter()
                    .map(|pm| Pubkey::new_from_array(pm.recipient))
                    .collect();

                let should_unlock = i == groups_count - 1;

                self.client.process_mint_group(
                    mint_buffer,
                    recipient_accounts,
                    i as u16,
                    mint_bump,
                    should_unlock,
                ).await?;

                println!("Processed mint group {}/{}", i + 1, groups_count);
            }
        }

        Ok(())
    }

    /// Mine a reorg chain (multiple blocks at once)
    pub async fn mine_reorg_chain(
        &mut self,
        blocks: Vec<Vec<BTAutoClaimedDeposit>>,
    ) -> Result<()> {
        if blocks.is_empty() {
            return Ok(());
        }

        // Read current state
        let bridge_state = self.read_bridge_state().await?;

        let start_height = bridge_state.core_state.bridge_header.finalized_state.block_height + 1;

        // Prepare all blocks
        let mut block_infos = Vec::new();
        let mut block_mints_data = Vec::new();
        let mut block_txo_indices = Vec::new();
        let mut total_new_deposits = 0;

        for deposits in &blocks {
            let (mints, mint_hash, txo_hash) = self.prepare_block_data_offline(deposits).await?;
            let indices: Vec<u32> = deposits.iter().map(|d| d.txo_index).collect();

            block_infos.push(FinalizedBlockMintTxoInfo {
                pending_mints_finalized_hash: mint_hash,
                txo_output_list_finalized_hash: txo_hash,
            });
            block_mints_data.push(mints);
            block_txo_indices.push(indices);
            total_new_deposits += deposits.len() as u32;
        }

        // Find first non-empty block
        let first_non_empty_idx = block_infos
            .iter()
            .position(|info| !info.is_empty())
            .unwrap_or(0);

        let target_height = start_height + first_non_empty_idx as u32;

        // Create buffers for first non-empty block
        let (mint_buffer, mint_bump) = self.client
            .create_pending_mint_buffer(
                self.client.bridge_state_pda,
                &block_mints_data[first_non_empty_idx],
            )
            .await?;

        self.current_txo_batch_id += 1;
        let (txo_buffer, txo_bump) = self.client
            .create_txo_buffer(target_height, &block_txo_indices[first_non_empty_idx])
            .await?;

        // Build new header
        let mut new_header = bridge_state.core_state.bridge_header.clone();
        new_header.finalized_state.block_height = start_height + blocks.len() as u32 - 1;
        new_header.tip_state.block_height = new_header.finalized_state.block_height;

        let last_info = block_infos.last().unwrap();
        new_header.finalized_state.pending_mints_finalized_hash = last_info.pending_mints_finalized_hash;
        new_header.finalized_state.txo_output_list_finalized_hash = last_info.txo_output_list_finalized_hash;
        new_header.finalized_state.auto_claimed_deposits_next_index += total_new_deposits;

        new_header.tip_state = PsyBridgeTipStateCommitment {
            block_hash: [1u8; 32],
            block_merkle_tree_root: [1u8; 32],
            block_time: new_header.tip_state.block_time + (blocks.len() as u32 * 60),
            block_height: new_header.tip_state.block_height,
        };

        // Extra blocks for reorg (all except the last one)
        let extra_blocks: Vec<FinalizedBlockMintTxoInfo> = block_infos
            .iter()
            .take(block_infos.len() - 1)
            .cloned()
            .collect();

        let extra_blocks_refs: Vec<&FinalizedBlockMintTxoInfo> = extra_blocks.iter().collect();

        // Generate proof
        let pub_inputs = get_reorg_block_transition_public_inputs(
            &bridge_state.core_state.bridge_header.get_hash_canonical(),
            &new_header.get_hash_canonical(),
            &extra_blocks_refs,
            &bridge_state.core_state.config_params.get_hash(),
            &bridge_state.core_state.custodian_wallet_config_hash,
        );
        let proof = generate_block_update_reorg_fake_proof(pub_inputs);

        println!(
            "Processing reorg: {} blocks from height {} to {}",
            blocks.len(),
            start_height,
            new_header.finalized_state.block_height
        );

        // Send reorg transaction
        self.client.send_reorg_blocks(
            proof,
            new_header,
            extra_blocks,
            mint_buffer,
            txo_buffer,
            mint_bump,
            txo_bump,
        ).await?;

        println!("Reorg blocks sent successfully");

        // Process mints for each non-empty block
        for i in first_non_empty_idx..block_infos.len() {
            let pending_mints = &block_mints_data[i];

            if pending_mints.is_empty() {
                continue;
            }

            // For blocks after the first, create new buffers
            if i > first_non_empty_idx {
                let this_height = start_height + i as u32;
                println!("Preparing Block {}: {} Deposits", this_height, pending_mints.len());

                self.client
                    .create_pending_mint_buffer(self.client.bridge_state_pda, pending_mints)
                    .await?;

                self.current_txo_batch_id += 1;
                self.client
                    .create_txo_buffer(this_height, &block_txo_indices[i])
                    .await?;
            }

            let groups_count = (pending_mints.len() + PM_MAX_PENDING_MINTS_PER_GROUP - 1)
                / PM_MAX_PENDING_MINTS_PER_GROUP;

            for g in 0..groups_count {
                let start = g * PM_MAX_PENDING_MINTS_PER_GROUP;
                let end = std::cmp::min(start + PM_MAX_PENDING_MINTS_PER_GROUP, pending_mints.len());
                let group_mints = &pending_mints[start..end];

                let recipient_accounts: Vec<Pubkey> = group_mints
                    .iter()
                    .map(|pm| Pubkey::new_from_array(pm.recipient))
                    .collect();

                let should_unlock = g == groups_count - 1;

                // Use auto-advance for all blocks in reorg
                self.client.process_mint_group_auto_advance(
                    mint_buffer,
                    txo_buffer,
                    recipient_accounts,
                    g as u16,
                    mint_bump,
                    txo_bump,
                    should_unlock,
                ).await?;

                println!(
                    "Processed mint group {}/{} for block {}",
                    g + 1,
                    groups_count,
                    start_height + i as u32
                );
            }
        }

        println!("Reorg chain processed successfully");
        Ok(())
    }
}
