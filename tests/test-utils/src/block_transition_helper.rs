use std::collections::HashMap;

use doge_bridge::state::BridgeState;
use doge_bridge_client::instructions::{
    block_update, process_mint_group, process_mint_group_auto_advance, process_reorg_blocks,
};
use psy_bridge_core::{crypto::hash::sha256_impl::hash_impl_sha256_bytes, error::QDogeResult, header::PsyBridgeTipStateCommitment};
use psy_doge_solana_core::{
    data_accounts::pending_mint::{PendingMint, PM_MAX_PENDING_MINTS_PER_GROUP},
    generic_cpi::{AutoClaimMintBufferAddressHelper, LockAutoClaimMintBufferCPIHelper},
    program_state::{compute_mint_group_info, FinalizedBlockMintTxoInfo},
    public_inputs::{get_block_transition_public_inputs, get_reorg_block_transition_public_inputs},
};

use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

use crate::{
    mock_data::{generate_block_update_fake_proof, generate_block_update_reorg_fake_proof},
    pending_mints_buffer_builder::PendingMintsGroupsBufferBuilder,
    test_client::TestBridgeClient,
};

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

pub struct BlockTransitionHelper {
    pub bridge_state: BridgeState,
    pub client: TestBridgeClient,
    pub user_accounts: HashMap<Pubkey, Keypair>,
    pub current_txo_batch_id: u32,
}
impl BlockTransitionHelper {
    pub fn get_user_account(&mut self, user_pubkey: &Pubkey) -> &Keypair {
        self.user_accounts.get(user_pubkey).unwrap()
    }
    pub fn add_user(&mut self) -> Pubkey {
        let user = Keypair::new();
        let user_pubkey = user.pubkey();
        self.user_accounts.insert(user_pubkey, user);
        user_pubkey
    }
}

pub struct MockMintBufferLocker {
    pub account_address: [u8; 32],
}
impl AutoClaimMintBufferAddressHelper for MockMintBufferLocker {
    fn get_mint_buffer_program_address(&self) -> [u8; 32] {
        self.account_address
    }
    fn is_pda_of_correct_auto_claim_deposit_mint_buffer_program(&self) -> bool {
        true
    }
}
impl LockAutoClaimMintBufferCPIHelper for MockMintBufferLocker {
    fn lock_buffer(&self) -> QDogeResult<()> {
        Ok(())
    }
}

impl BlockTransitionHelper {
    pub async fn new_from_client(client: TestBridgeClient) -> anyhow::Result<Self> {
        let bridge_account = client
            .client
            .get_account(client.bridge_state_pda)
            .await?
            .unwrap();

        let bridge_state: &BridgeState = bytemuck::from_bytes(&bridge_account.data);
        let bridge_state = bridge_state.clone();

        Ok(Self {
            bridge_state,
            client,
            user_accounts: HashMap::new(),
            current_txo_batch_id: 0,
        })
    }

    async fn prepare_block_data_offline(
        &mut self,
        deposits: &[BTAutoClaimedDeposit],
    ) -> (Vec<PendingMint>, [u8; 32], [u8; 32]) {
        let mut pending_mints = Vec::with_capacity(deposits.len());
        for d in deposits {
            let user_pubkey = Pubkey::new_from_array(d.depositor_pubkey);

            let user_kp_ref = self.user_accounts.get(&user_pubkey).unwrap();
            let user_kp = Keypair::from_bytes(&user_kp_ref.to_bytes()).unwrap();

            self.client
                .create_token_ata_if_needed(self.client.doge_mint, &user_kp)
                .await;

            let user_ata = spl_associated_token_account::get_associated_token_address(
                &user_pubkey,
                &self.client.doge_mint,
            );
            pending_mints.push(PendingMint {
                recipient: user_ata.to_bytes(),
                amount: d.amount,
            });
        }

        let txo_indices: Vec<u32> = deposits.iter().map(|d| d.txo_index).collect();
        let txo_bytes: Vec<u8> = txo_indices.iter().flat_map(|x| x.to_le_bytes()).collect();
        let txo_hash = hash_impl_sha256_bytes(&txo_bytes);

        let mut pending_mints_builder =
            PendingMintsGroupsBufferBuilder::new_with_hint(pending_mints.len());
        for pm in &pending_mints {
            pending_mints_builder.append_pending_mint(&pm.recipient, pm.amount);
        }

        let pending_mints_hash = if pending_mints.is_empty() {
            [
                150, 162, 150, 210, 36, 242, 133, 198, 123, 238, 147, 195, 15, 138, 48, 145, 87,
                240, 218, 163, 93, 197, 184, 126, 65, 11, 120, 99, 10, 9, 207, 199,
            ]
        } else {
            pending_mints_builder.finalize().unwrap().finalized_hash
        };

        (pending_mints, pending_mints_hash, txo_hash)
    }

    pub async fn mine_and_process_block(
        &mut self,
        auto_claimed_deposits: Vec<BTAutoClaimedDeposit>,
    ) -> anyhow::Result<()> {
        let (pending_mints, pending_mints_hash, txo_buffer_hash) = self
            .prepare_block_data_offline(&auto_claimed_deposits)
            .await;

        let txo_indices: Vec<u32> = auto_claimed_deposits.iter().map(|d| d.txo_index).collect();

        let mut new_header = self.bridge_state.core_state.bridge_header.clone();
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

        let pub_inputs = get_block_transition_public_inputs(
            &self
                .bridge_state
                .core_state
                .bridge_header
                .get_hash_canonical(),
            &new_header.get_hash_canonical(),
            &self.bridge_state.core_state.config_params.get_hash(),
            &self.bridge_state.core_state.custodian_wallet_config.get_wallet_config_hash(),
        );
        let proof = generate_block_update_fake_proof(pub_inputs);

        let new_height = new_header.finalized_state.block_height;
        let pending_mint_buffer_pubkey = self
            .client
            .create_pending_mint_buffer(self.client.bridge_state_pda, &pending_mints)
            .await;
        self.current_txo_batch_id += 1;
        let txo_buffer_pubkey = self
            .client
            .create_txo_buffer(new_height, &txo_indices, self.current_txo_batch_id)
            .await;

        let payer_pubkey = self.client.payer.pubkey().to_bytes();
        let (_, mint_bump) = Pubkey::find_program_address(
            &[b"mint_buffer", &payer_pubkey],
            &self.client.pending_mint_program_id,
        );
        let (_, txo_bump) = Pubkey::find_program_address(
            &[b"txo_buffer", &payer_pubkey],
            &self.client.txo_buffer_program_id,
        );

        println!(
            "Mining Block {}: {} Deposits",
            new_height,
            pending_mints.len()
        );

        let update_ix = block_update(
            self.client.program_id,
            self.client.payer.pubkey(),
            proof,
            new_header,
            self.client.operator.pubkey(),
            pending_mint_buffer_pubkey,
            txo_buffer_pubkey,
            mint_bump,
            txo_bump,
        );
        println!("Sending Block Update Transaction...");
        self.client.send_tx(&[update_ix], &[]).await;
        println!("Sent block update transaction.");
        self.bridge_state.core_state.bridge_header = new_header;

        // FIX: Only call standard_append_block in local state if mints > 0, to match chain logic
        if !pending_mints.is_empty() {
            let (mint_groups, _) = compute_mint_group_info(pending_mints.len() as u16);
            self.bridge_state
                .core_state
                .pending_mint_txos
                .standard_append_block(
                    new_height,
                    &[&FinalizedBlockMintTxoInfo {
                        pending_mints_finalized_hash: pending_mints_hash,
                        txo_output_list_finalized_hash: txo_buffer_hash,
                    }],
                    pending_mint_buffer_pubkey.to_bytes(),
                    pending_mints.len() as u32,
                    mint_groups as u32,
                )?;

            let groups_count = (pending_mints.len() + PM_MAX_PENDING_MINTS_PER_GROUP - 1)
                / PM_MAX_PENDING_MINTS_PER_GROUP;
            for i in 0..groups_count {
                let start = i * PM_MAX_PENDING_MINTS_PER_GROUP;
                let end =
                    std::cmp::min(start + PM_MAX_PENDING_MINTS_PER_GROUP, pending_mints.len());
                let group_mints = &pending_mints[start..end];

                let recipient_accounts: Vec<Pubkey> = group_mints
                    .iter()
                    .map(|pm| Pubkey::new_from_array(pm.recipient))
                    .collect();
                let should_unlock = i == groups_count - 1;

                let process_ix = process_mint_group(
                    self.client.program_id,
                    self.client.operator.pubkey(),
                    pending_mint_buffer_pubkey,
                    self.client.doge_mint,
                    recipient_accounts,
                    i as u16,
                    mint_bump,
                    should_unlock,
                );
                self.client.send_tx(&[process_ix], &[]).await;

                self.bridge_state
                    .core_state
                    .pending_mint_txos
                    .mark_pending_mints_group_claimed(i as u16)?;
            }
        }
        Ok(())
    }

    pub async fn mine_reorg_chain(
        &mut self,
        blocks: Vec<Vec<BTAutoClaimedDeposit>>,
    ) -> anyhow::Result<()> {
        if blocks.is_empty() {
            return Ok(());
        }

        let start_height = self
            .bridge_state
            .core_state
            .bridge_header
            .finalized_state
            .block_height
            + 1;

        let mut block_infos = Vec::new();
        let mut block_mints_data = Vec::new();
        let mut block_txo_indices = Vec::new();
        let mut total_new_deposits = 0;

        for deposits in &blocks {
            let (mints, mint_hash, txo_hash) = self.prepare_block_data_offline(deposits).await;
            let indices: Vec<u32> = deposits.iter().map(|d| d.txo_index).collect();
            block_infos.push(FinalizedBlockMintTxoInfo {
                pending_mints_finalized_hash: mint_hash,
                txo_output_list_finalized_hash: txo_hash,
            });
            block_mints_data.push(mints);
            block_txo_indices.push(indices);
            total_new_deposits += deposits.len() as u32;
        }

        let first_non_empty_idx = block_infos
            .iter()
            .position(|info| !info.is_empty())
            .unwrap_or(0);

        let target_height = start_height + first_non_empty_idx as u32;
        let mint_buffer_pk = self
            .client
            .create_pending_mint_buffer(
                self.client.bridge_state_pda,
                &block_mints_data[first_non_empty_idx],
            )
            .await;
        self.current_txo_batch_id += 1;
        let txo_buffer_pk = self
            .client
            .create_txo_buffer(
                target_height,
                &block_txo_indices[first_non_empty_idx],
                self.current_txo_batch_id,
            )
            .await;

        let payer_pubkey = self.client.payer.pubkey().to_bytes();
        let (_, mint_bump) = Pubkey::find_program_address(
            &[b"mint_buffer", &payer_pubkey],
            &self.client.pending_mint_program_id,
        );
        let (_, txo_bump) = Pubkey::find_program_address(
            &[b"txo_buffer", &payer_pubkey],
            &self.client.txo_buffer_program_id,
        );

        let mut new_header = self.bridge_state.core_state.bridge_header.clone();
        new_header.finalized_state.block_height = start_height + blocks.len() as u32 - 1;
        new_header.tip_state.block_height = new_header.finalized_state.block_height;
        let last_info = block_infos.last().unwrap();
        new_header.finalized_state.pending_mints_finalized_hash =
            last_info.pending_mints_finalized_hash;
        new_header.finalized_state.txo_output_list_finalized_hash =
            last_info.txo_output_list_finalized_hash;
        new_header.finalized_state.auto_claimed_deposits_next_index += total_new_deposits;
        new_header.tip_state = PsyBridgeTipStateCommitment {
            block_hash: [1u8; 32],
            block_merkle_tree_root: [1u8; 32],
            block_time: new_header.tip_state.block_time + (blocks.len() as u32 * 60),
            block_height: new_header.tip_state.block_height,
        };

        let extra_blocks_refs: Vec<&FinalizedBlockMintTxoInfo> =
            block_infos.iter().take(block_infos.len() - 1).collect();
        let extra_blocks_owned: Vec<FinalizedBlockMintTxoInfo> =
            extra_blocks_refs.iter().map(|&x| x.clone()).collect();

        let pub_inputs = get_reorg_block_transition_public_inputs(
            &self
                .bridge_state
                .core_state
                .bridge_header
                .get_hash_canonical(),
            &new_header.get_hash_canonical(),
            &extra_blocks_refs,
            &self.bridge_state.core_state.config_params.get_hash(),
            &self.bridge_state.core_state.custodian_wallet_config.get_wallet_config_hash(),
        );
        let proof = generate_block_update_reorg_fake_proof(pub_inputs);

        let reorg_ix = process_reorg_blocks(
            self.client.program_id,
            self.client.payer.pubkey(),
            proof,
            new_header,
            extra_blocks_owned,
            self.client.operator.pubkey(),
            mint_buffer_pk,
            txo_buffer_pk,
            mint_bump,
            txo_bump,
        );
        self.client.send_tx(&[reorg_ix], &[]).await;

        self.bridge_state.core_state.bridge_header = new_header;
        let slice: Vec<&FinalizedBlockMintTxoInfo> =
            block_infos.iter().skip(first_non_empty_idx).collect();
        let (mint_groups, _) =
            compute_mint_group_info(block_mints_data[first_non_empty_idx].len() as u16);
        self.bridge_state
            .core_state
            .pending_mint_txos
            .standard_append_block(
                start_height + first_non_empty_idx as u32,
                &slice,
                mint_buffer_pk.to_bytes(),
                block_mints_data[first_non_empty_idx].len() as u32,
                mint_groups as u32,
            )?;

        for i in first_non_empty_idx..block_infos.len() {
            let pending_mints = &block_mints_data[i];

            if pending_mints.is_empty() {
                continue;
            }

            if i > first_non_empty_idx {
                println!(
                    "Preparing Block {}: {} Deposits",
                    start_height + i as u32,
                    pending_mints.len()
                );
                let this_height = start_height + i as u32;
                self.client
                    .create_pending_mint_buffer(self.client.bridge_state_pda, pending_mints)
                    .await;
                self.current_txo_batch_id += 1;
                self.client
                    .create_txo_buffer(
                        this_height,
                        &block_txo_indices[i],
                        self.current_txo_batch_id,
                    )
                    .await;
            }

            let groups_count = (pending_mints.len() + PM_MAX_PENDING_MINTS_PER_GROUP - 1)
                / PM_MAX_PENDING_MINTS_PER_GROUP;

            for g in 0..groups_count {
                let start = g * PM_MAX_PENDING_MINTS_PER_GROUP;
                let end =
                    std::cmp::min(start + PM_MAX_PENDING_MINTS_PER_GROUP, pending_mints.len());
                let group_mints = &pending_mints[start..end];

                let recipient_accounts: Vec<Pubkey> = group_mints
                    .iter()
                    .map(|pm| Pubkey::new_from_array(pm.recipient))
                    .collect();
                let should_unlock = g == groups_count - 1;

                let ix = if i == first_non_empty_idx {
                    process_mint_group_auto_advance(
                        self.client.program_id,
                        self.client.operator.pubkey(),
                        mint_buffer_pk,
                        txo_buffer_pk,
                        self.client.doge_mint,
                        recipient_accounts,
                        g as u16,
                        mint_bump,
                        txo_bump,
                        should_unlock,
                    )
                } else {
                    process_mint_group_auto_advance(
                        self.client.program_id,
                        self.client.operator.pubkey(),
                        mint_buffer_pk,
                        txo_buffer_pk,
                        self.client.doge_mint,
                        recipient_accounts,
                        g as u16,
                        mint_bump,
                        txo_bump,
                        should_unlock,
                    )
                };

                // FIX: Snapshot data BEFORE execution if we need it for state update
                // because execution might unlock/clear the buffer.
                let (mint_data_snapshot, txo_data_snapshot) = if i > first_non_empty_idx && g == 0 {
                    (
                        self.client
                            .client
                            .get_account(mint_buffer_pk)
                            .await
                            .unwrap()
                            .map(|a| a.data),
                        self.client
                            .client
                            .get_account(txo_buffer_pk)
                            .await
                            .unwrap()
                            .map(|a| a.data),
                    )
                } else {
                    (None, None)
                };

                self.client.send_tx(&[ix], &[]).await;

                if let (Some(mint_data), Some(txo_data)) = (mint_data_snapshot, txo_data_snapshot) {
                    let locker = MockMintBufferLocker {
                        account_address: mint_buffer_pk.to_bytes(),
                    };

                    self.bridge_state.core_state.run_setup_next_pending_buffer(
                        &self.client.bridge_state_pda.to_bytes(),
                        locker.get_mint_buffer_program_address(),
                        &txo_data,
                        &mint_data,
                    )?;
                }

                self.bridge_state
                    .core_state
                    .pending_mint_txos
                    .mark_pending_mints_group_claimed(g as u16)?;
            }
        }

        Ok(())
    }
}
