use crate::{
    buffer_manager::BufferManager,
    constants::{
        DOGE_BRIDGE_PROGRAM_ID, GENERIC_BUFFER_BUILDER_PROGRAM_ID, MANUAL_CLAIM_PROGRAM_ID,
        PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID, TXO_BUFFER_BUILDER_PROGRAM_ID,
    },
    errors::ClientError,
    instructions,
};
use psy_bridge_core::{
    crypto::{hash::sha256_impl::hash_impl_sha256_bytes, zk::CompactBridgeZKProof}, header::PsyBridgeHeaderUpdate}
;
use psy_doge_solana_core::{
    data_accounts::pending_mint::{PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH, PM_TXO_DEFAULT_BUFFER_HASH, PendingMint}, instructions::doge_bridge::InitializeBridgeParams, program_state::{FinalizedBlockMintTxoInfo, PsyBridgeProgramState, PsyReturnTxOutput}
};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::sync::Arc;

fn compute_mints_hash(mints: &[PendingMint]) -> [u8; 32] {
    if mints.is_empty() {
        return PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH;
    }
    let count = mints.len() as u16;
    let group_size = 24;
    let num_groups = (mints.len() + group_size - 1) / group_size;
    let mut preimage = Vec::new();
    preimage.extend_from_slice(&count.to_le_bytes());
    for i in 0..num_groups {
        let start = i * group_size;
        let end = std::cmp::min(start + group_size, mints.len());
        let mut group_bytes = Vec::new();
        for m in &mints[start..end] {
            group_bytes.extend_from_slice(bytemuck::bytes_of(m));
        }
        preimage.extend_from_slice(&hash_impl_sha256_bytes(&group_bytes));
    }
    hash_impl_sha256_bytes(&preimage)
}

fn compute_txo_hash(indices: &[u32]) -> [u8; 32] {
    if indices.is_empty() {
        return PM_TXO_DEFAULT_BUFFER_HASH;
    }
    let bytes: Vec<u8> = indices.iter().flat_map(|x| x.to_le_bytes()).collect();
    hash_impl_sha256_bytes(&bytes)
}

pub struct DogeBridgeClient {
    pub client: Arc<RpcClient>,
    pub payer: Keypair,
    pub operator: Keypair,
    pub fee_spender: Keypair,
    pub program_id: Pubkey,
    pub manual_claim_program_id: Pubkey,
    pub pending_mint_program_id: Pubkey,
    pub txo_buffer_program_id: Pubkey,
    pub generic_buffer_program_id: Pubkey,
    pub wormhole_core_program_id: Pubkey,
    pub wormhole_shim_program_id: Pubkey,
    pub doge_mint: Pubkey,
}

impl DogeBridgeClient {
    pub fn new(
        client: Arc<RpcClient>,
        payer: Keypair,
        operator: Keypair,
        fee_spender: Keypair,
        doge_mint: Pubkey,
        wormhole_core_program_id: Pubkey,
        wormhole_shim_program_id: Pubkey,
    ) -> Self {
        Self {
            client,
            payer,
            program_id: DOGE_BRIDGE_PROGRAM_ID,
            manual_claim_program_id: MANUAL_CLAIM_PROGRAM_ID,
            pending_mint_program_id: PENDING_MINT_BUFFER_BUILDER_PROGRAM_ID,
            txo_buffer_program_id: TXO_BUFFER_BUILDER_PROGRAM_ID,
            generic_buffer_program_id: GENERIC_BUFFER_BUILDER_PROGRAM_ID,
            doge_mint,
            operator,
            fee_spender,
            wormhole_core_program_id,
            wormhole_shim_program_id,
        }
    }

    fn buffer_manager(&self) -> BufferManager {
        BufferManager::new(
            self.client.clone(),
            Keypair::from_bytes(&self.payer.to_bytes()).unwrap(),
        )
    }

    pub async fn initialize(&self, params: &InitializeBridgeParams) -> Result<(), ClientError> {
        let ix = instructions::initialize_bridge(
            self.payer.pubkey(),
            self.operator.pubkey(),
            self.fee_spender.pubkey(),
            self.doge_mint,
            &params,
        );
        self.buffer_manager().send_tx(&[ix], &[]).await
    }

    pub fn block_update_builder<'a>(
        &'a self,
        current_state: &'a PsyBridgeProgramState,
        required_confirmations: u32,
    ) -> BlockUpdateBuilder<'a> {
        BlockUpdateBuilder::new(self, current_state, required_confirmations)
    }

    pub async fn request_withdrawal(
        &self,
        mint: Pubkey,
        user_token_account: Pubkey,
        user_authority: &Keypair,
        recipient_address: [u8; 20],
        amount_sats: u64,
        address_type: u32,
    ) -> Result<(), ClientError> {
        let ix = instructions::request_withdrawal(
            self.program_id,
            user_authority.pubkey(),
            mint,
            user_token_account,
            recipient_address,
            amount_sats,
            address_type,
        );
        self.buffer_manager()
            .send_tx(&[ix], &[user_authority])
            .await
    }

    pub async fn process_withdrawal(
        &self,
        proof: CompactBridgeZKProof,
        new_return_output: PsyReturnTxOutput,
        new_spent_txo_tree_root: [u8; 32],
        new_next_processed_withdrawals_index: u64,
        new_total_spent_deposit_utxo_count: u64,
        doge_tx_bytes: &[u8],
    ) -> Result<(), ClientError> {
        let buffer = self
            .buffer_manager()
            .create_generic_buffer(self.generic_buffer_program_id, doge_tx_bytes)
            .await?;
        let ix = instructions::process_withdrawal(
            self.program_id,
            self.payer.pubkey(),
            buffer,
            self.wormhole_shim_program_id,
            self.wormhole_core_program_id,
            proof,
            new_return_output,
            new_spent_txo_tree_root,
            new_next_processed_withdrawals_index,
            new_total_spent_deposit_utxo_count,
        );
        self.buffer_manager().send_tx(&[ix], &[]).await
    }
}

pub struct BlockUpdateBuilder<'a> {
    client: &'a DogeBridgeClient,
    current_state: &'a PsyBridgeProgramState,
    header_update: Option<PsyBridgeHeaderUpdate>,
    proof: Option<CompactBridgeZKProof>,
    new_doge_block_height: Option<u32>,
    pending_mints: &'a [PendingMint],
    txo_indices: &'a [u32],
    extra_blocks: Vec<FinalizedBlockMintTxoInfo>,
    pub required_confirmations: u32,
}

impl<'a> BlockUpdateBuilder<'a> {
    pub fn new(client: &'a DogeBridgeClient, current_state: &'a PsyBridgeProgramState, required_confirmations: u32) -> Self {
        Self {
            client,
            current_state,
            proof: None,
            new_doge_block_height: None,
            pending_mints: &[],
            txo_indices: &[],
            extra_blocks: vec![],
            required_confirmations,
            header_update: None,
        }
    }
    pub fn with_header_update(mut self, header_update: PsyBridgeHeaderUpdate) -> Self {
        self.header_update = Some(header_update);
        self
    }

    pub fn with_proof(mut self, proof: CompactBridgeZKProof) -> Self {
        self.proof = Some(proof);
        self
    }
    pub fn with_new_doge_block_height(mut self, height: u32) -> Self {
        self.new_doge_block_height = Some(height);
        self
    }
    pub fn with_pending_mints(mut self, mints: &'a [PendingMint]) -> Self {
        self.pending_mints = mints;
        self
    }
    pub fn with_txo_indices(mut self, indices: &'a [u32]) -> Self {
        self.txo_indices = indices;
        self
    }
    pub fn with_reorg_blocks(mut self, blocks: Vec<FinalizedBlockMintTxoInfo>) -> Self {
        self.extra_blocks = blocks;
        self
    }

    pub async fn execute(self) -> Result<(), ClientError> {
        let proof = self
            .proof
            .ok_or_else(|| ClientError::InvalidInput("Proof is required".to_string()))?;
        let new_height = self
            .new_doge_block_height
            .ok_or_else(|| ClientError::InvalidInput("New block height is required".to_string()))?;

        let header_update = self.header_update
            .ok_or_else(|| ClientError::InvalidInput("Header update is required".to_string()))?;
        if new_height < self.current_state.bridge_header.finalized_state.block_height + self.required_confirmations {
            return Err(ClientError::InvalidInput("New block height does not satisfy required confirmations".to_string()));
        }

        let pending_mints_hash = compute_mints_hash(self.pending_mints);
        let txo_hash = compute_txo_hash(self.txo_indices);
        let auto_claimed_deposits_next_index = self.current_state.bridge_header.finalized_state.auto_claimed_deposits_next_index + self.pending_mints.len() as u32;

        let mut new_header = header_update.to_header(self.required_confirmations, pending_mints_hash, txo_hash, auto_claimed_deposits_next_index);
        new_header.finalized_state.block_height = new_height-self.required_confirmations;
        new_header.tip_state.block_height = new_height;
        new_header.finalized_state.pending_mints_finalized_hash = pending_mints_hash;
        new_header.finalized_state.txo_output_list_finalized_hash = txo_hash;
        new_header.tip_state = header_update.tip_state;

        let (bridge_state_pda, _) =
            Pubkey::find_program_address(&[b"bridge_state"], &self.client.program_id);
        let bm = self.client.buffer_manager();
        
        let (mint_buffer, mint_buffer_bump) = bm
            .create_pending_mint_buffer(
                self.client.pending_mint_program_id,
                bridge_state_pda,
                self.pending_mints,
            )
            .await?;
        let (txo_buffer, txo_buffer_bump) = bm
            .create_txo_buffer(
                self.client.txo_buffer_program_id,
                new_height,
                self.txo_indices,
            )
            .await?;

        let ix = if self.extra_blocks.is_empty() {
            instructions::block_update(
                self.client.program_id,
                self.client.payer.pubkey(),
                proof,
                new_header,
                self.client.operator.pubkey(),
                mint_buffer,
                txo_buffer,
                mint_buffer_bump,
                txo_buffer_bump,
            )
        } else {
            instructions::process_reorg_blocks(
                self.client.program_id,
                self.client.payer.pubkey(),
                proof,
                new_header,
                self.extra_blocks,
                self.client.operator.pubkey(),
                mint_buffer,
                txo_buffer,
                mint_buffer_bump,
                txo_buffer_bump,
            )
        };

        bm.send_tx(&[ix], &[]).await
    }
}