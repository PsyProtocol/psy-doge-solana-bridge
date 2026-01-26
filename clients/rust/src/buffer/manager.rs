//! Parallel buffer manager for efficient buffer creation.
//!
//! Provides parallel buffer building with rate limiting and retry logic.

use std::sync::Arc;

use futures::future::try_join_all;
use psy_doge_solana_core::data_accounts::pending_mint::PendingMint;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};

use crate::{
    config::ParallelismConfig,
    errors::BridgeError,
    instructions,
    rpc::{RpcRateLimiter, RetryExecutor},
};

use super::{
    pending_mint::{derive_pending_mint_buffer_pda, PendingMintBufferBuilder, PENDING_MINT_BUFFER_HEADER_SIZE},
    txo::{derive_txo_buffer_pda, TxoBufferBuilder, TXO_BUFFER_HEADER_SIZE},
    CHUNK_SIZE,
};

/// Parallel buffer manager for efficient buffer creation.
///
/// Handles creation of pending mint buffers and TXO buffers with
/// parallel operations where possible.
pub struct ParallelBufferManager {
    rpc: Arc<RpcClient>,
    payer: Arc<Keypair>,
    /// The operator keypair used for PDA derivation and signing buffer operations.
    /// Buffers are operator-specific and require operator signature for writes.
    operator: Arc<Keypair>,
    rate_limiter: Arc<RpcRateLimiter>,
    retry_executor: RetryExecutor,
    config: ParallelismConfig,
}

impl ParallelBufferManager {
    /// Create a new parallel buffer manager.
    ///
    /// # Arguments
    /// * `rpc` - RPC client for Solana
    /// * `payer` - Keypair that pays for transactions
    /// * `operator` - Operator keypair used for PDA derivation and signing buffer operations
    /// * `rate_limiter` - Rate limiter for RPC requests
    /// * `retry_executor` - Retry executor for failed requests
    /// * `config` - Parallelism configuration
    pub fn new(
        rpc: Arc<RpcClient>,
        payer: Arc<Keypair>,
        operator: Arc<Keypair>,
        rate_limiter: Arc<RpcRateLimiter>,
        retry_executor: RetryExecutor,
        config: ParallelismConfig,
    ) -> Self {
        Self {
            rpc,
            payer,
            operator,
            rate_limiter,
            retry_executor,
            config,
        }
    }

    /// Create a pending mint buffer with parallel group insertions.
    pub async fn create_pending_mint_buffer(
        &self,
        program_id: Pubkey,
        locker: Pubkey,
        mints: &[PendingMint],
    ) -> Result<(Pubkey, u8), BridgeError> {
        let (buffer_pubkey, bump) = derive_pending_mint_buffer_pda(&program_id, &self.operator.pubkey());

        // Ensure buffer exists
        self.ensure_pending_mint_buffer_exists(program_id, buffer_pubkey, locker)
            .await?;

        if mints.is_empty() {
            return Ok((buffer_pubkey, bump));
        }

        // Reinitialize with total count
        let total_mints = mints.len() as u16;
        self.reinit_pending_mint_buffer(program_id, buffer_pubkey, total_mints)
            .await?;

        // Build groups
        let builder = PendingMintBufferBuilder::new(mints.to_vec());
        let num_groups = builder.num_groups();

        // Insert groups in parallel batches
        for batch_start in (0..num_groups).step_by(self.config.group_batch_size) {
            let batch_end = std::cmp::min(batch_start + self.config.group_batch_size, num_groups);

            let futures: Vec<_> = (batch_start..batch_end)
                .map(|group_idx| {
                    let program_id = program_id;
                    let buffer_pubkey = buffer_pubkey;
                    let group_data = builder.serialize_group(group_idx);

                    self.insert_pending_mint_group(
                        program_id,
                        buffer_pubkey,
                        group_idx as u16,
                        group_data,
                    )
                })
                .collect();

            try_join_all(futures).await?;
        }

        Ok((buffer_pubkey, bump))
    }

    /// Ensure the pending mint buffer account exists.
    async fn ensure_pending_mint_buffer_exists(
        &self,
        program_id: Pubkey,
        buffer_pubkey: Pubkey,
        locker: Pubkey,
    ) -> Result<(), BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let account = self
            .rpc
            .get_account_with_commitment(&buffer_pubkey, CommitmentConfig::confirmed())
            .await?
            .value;

        if account.is_none() {
            // Create buffer account
            let space = PENDING_MINT_BUFFER_HEADER_SIZE;
            let rent = self
                .rpc
                .get_minimum_balance_for_rent_exemption(space)
                .await?;

            let transfer_ix =
                system_instruction::transfer(&self.payer.pubkey(), &buffer_pubkey, rent);
            let setup_ix = instructions::pending_mint_setup(
                program_id,
                buffer_pubkey,
                locker,
                self.operator.pubkey(),
            );

            self.send_and_confirm(&[transfer_ix, setup_ix]).await?;
        }

        Ok(())
    }

    /// Reinitialize the pending mint buffer.
    async fn reinit_pending_mint_buffer(
        &self,
        program_id: Pubkey,
        buffer_pubkey: Pubkey,
        total_mints: u16,
    ) -> Result<(), BridgeError> {
        let reinit_ix = instructions::pending_mint_reinit(
            program_id,
            buffer_pubkey,
            self.operator.pubkey(),
            total_mints,
        );
        self.send_and_confirm_with_operator(&[reinit_ix]).await?;
        Ok(())
    }

    /// Insert a single pending mint group.
    async fn insert_pending_mint_group(
        &self,
        program_id: Pubkey,
        buffer_pubkey: Pubkey,
        group_idx: u16,
        group_data: Vec<u8>,
    ) -> Result<(), BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let operator_pubkey = self.operator.pubkey();

        self.retry_executor
            .execute(|| {
                let insert_ix = instructions::pending_mint_insert(
                    program_id,
                    buffer_pubkey,
                    operator_pubkey,
                    group_idx,
                    &group_data,
                );
                self.send_tx_with_operator(vec![insert_ix])
            })
            .await
    }

    /// Create a TXO buffer with parallel chunk writes.
    pub async fn create_txo_buffer(
        &self,
        program_id: Pubkey,
        block_height: u32,
        txo_indices: &[u32],
    ) -> Result<(Pubkey, u8), BridgeError> {
        let (buffer_pubkey, bump) = derive_txo_buffer_pda(&program_id, &self.operator.pubkey());

        // Get or create buffer and determine batch_id
        let batch_id = self
            .ensure_txo_buffer_exists(program_id, buffer_pubkey)
            .await?;

        let builder = TxoBufferBuilder::new(txo_indices.to_vec(), block_height);
        let data_size = builder.data_size() as u32;

        // Set length with resize
        self.set_txo_buffer_length(
            program_id,
            buffer_pubkey,
            data_size,
            batch_id,
            block_height,
            true,  // resize
            false, // don't finalize yet
        )
        .await?;

        // Write chunks in parallel batches
        let chunks = builder.chunks();

        for batch_start in (0..chunks.len()).step_by(self.config.max_concurrent_writes) {
            let batch_end =
                std::cmp::min(batch_start + self.config.max_concurrent_writes, chunks.len());

            let futures: Vec<_> = chunks[batch_start..batch_end]
                .iter()
                .map(|(offset, data)| {
                    self.write_txo_buffer_chunk(
                        program_id,
                        buffer_pubkey,
                        batch_id,
                        *offset as u32,
                        data.clone(),
                    )
                })
                .collect();

            try_join_all(futures).await?;
        }

        // Finalize buffer
        self.set_txo_buffer_length(
            program_id,
            buffer_pubkey,
            data_size,
            batch_id,
            block_height,
            false, // no resize
            true,  // finalize
        )
        .await?;

        Ok((buffer_pubkey, bump))
    }

    /// Ensure the TXO buffer exists and return the batch_id to use.
    async fn ensure_txo_buffer_exists(
        &self,
        program_id: Pubkey,
        buffer_pubkey: Pubkey,
    ) -> Result<u32, BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let account = self
            .rpc
            .get_account_with_commitment(&buffer_pubkey, CommitmentConfig::confirmed())
            .await?
            .value;

        if let Some(account_data) = account {
            // Buffer exists, get current batch_id and increment
            if account_data.data.len() >= 44 {
                let batch_id = u32::from_le_bytes(
                    account_data.data[40..44]
                        .try_into()
                        .map_err(|_| BridgeError::buffer_failed("Invalid buffer header"))?,
                );
                Ok(batch_id + 1)
            } else {
                Err(BridgeError::buffer_failed("Invalid buffer data"))
            }
        } else {
            // Create buffer account
            let space = TXO_BUFFER_HEADER_SIZE;
            let rent = self
                .rpc
                .get_minimum_balance_for_rent_exemption(space)
                .await?;

            let transfer_ix =
                system_instruction::transfer(&self.payer.pubkey(), &buffer_pubkey, rent);
            let init_ix =
                instructions::txo_buffer_init(program_id, buffer_pubkey, self.operator.pubkey());

            self.send_and_confirm(&[transfer_ix, init_ix]).await?;
            Ok(0)
        }
    }

    /// Set TXO buffer length.
    async fn set_txo_buffer_length(
        &self,
        program_id: Pubkey,
        buffer_pubkey: Pubkey,
        data_size: u32,
        batch_id: u32,
        block_height: u32,
        resize: bool,
        finalize: bool,
    ) -> Result<(), BridgeError> {
        let ix = instructions::txo_buffer_set_len(
            program_id,
            buffer_pubkey,
            self.payer.pubkey(),
            self.operator.pubkey(),
            data_size,
            resize,
            batch_id,
            block_height,
            finalize,
        );
        self.send_and_confirm_with_operator(&[ix]).await
    }

    /// Write a chunk to the TXO buffer.
    async fn write_txo_buffer_chunk(
        &self,
        program_id: Pubkey,
        buffer_pubkey: Pubkey,
        batch_id: u32,
        offset: u32,
        data: Vec<u8>,
    ) -> Result<(), BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let operator_pubkey = self.operator.pubkey();

        self.retry_executor
            .execute(|| {
                let write_ix = instructions::txo_buffer_write(
                    program_id,
                    buffer_pubkey,
                    operator_pubkey,
                    batch_id,
                    offset,
                    &data,
                );
                self.send_tx_with_operator(vec![write_ix])
            })
            .await
    }

    /// Create a generic buffer for arbitrary data.
    pub async fn create_generic_buffer(
        &self,
        program_id: Pubkey,
        data: &[u8],
    ) -> Result<Pubkey, BridgeError> {
        let buffer_account = Keypair::new();
        let buffer_pubkey = buffer_account.pubkey();
        let target_size = data.len() as u32;

        // Create account
        let space = 32; // Header size
        let rent = self
            .rpc
            .get_minimum_balance_for_rent_exemption(space)
            .await?;

        let create_ix = system_instruction::create_account(
            &self.payer.pubkey(),
            &buffer_pubkey,
            rent,
            space as u64,
            &program_id,
        );

        let init_ix = instructions::generic_buffer_init(
            program_id,
            buffer_pubkey,
            self.payer.pubkey(),
            target_size,
        );

        self.send_and_confirm_with_signer(&[create_ix, init_ix], &buffer_account)
            .await?;

        // Write data in chunks (parallel batches)
        let chunks: Vec<_> = data.chunks(CHUNK_SIZE).enumerate().collect();

        for batch_start in (0..chunks.len()).step_by(self.config.max_concurrent_writes) {
            let batch_end =
                std::cmp::min(batch_start + self.config.max_concurrent_writes, chunks.len());

            let futures: Vec<_> = chunks[batch_start..batch_end]
                .iter()
                .map(|(i, chunk)| {
                    let offset = (*i * CHUNK_SIZE) as u32;
                    self.write_generic_buffer_chunk(program_id, buffer_pubkey, offset, chunk.to_vec())
                })
                .collect();

            try_join_all(futures).await?;
        }

        Ok(buffer_pubkey)
    }

    /// Write a chunk to a generic buffer.
    async fn write_generic_buffer_chunk(
        &self,
        program_id: Pubkey,
        buffer_pubkey: Pubkey,
        offset: u32,
        data: Vec<u8>,
    ) -> Result<(), BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let payer_pubkey = self.payer.pubkey();

        self.retry_executor
            .execute(|| {
                let write_ix = instructions::generic_buffer_write(
                    program_id,
                    buffer_pubkey,
                    payer_pubkey,
                    offset,
                    &data,
                );
                self.send_tx(vec![write_ix])
            })
            .await
    }

    /// Send a transaction and wait for confirmation (payer signs only).
    async fn send_and_confirm(&self, instructions: &[Instruction]) -> Result<(), BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let instructions = instructions.to_vec();
        self.retry_executor
            .execute(|| self.send_tx(instructions.clone()))
            .await
    }

    /// Send a transaction and wait for confirmation (payer and operator sign).
    async fn send_and_confirm_with_operator(&self, instructions: &[Instruction]) -> Result<(), BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let instructions = instructions.to_vec();
        self.retry_executor
            .execute(|| self.send_tx_with_operator(instructions.clone()))
            .await
    }

    /// Send a transaction with an additional signer.
    async fn send_and_confirm_with_signer(
        &self,
        instructions: &[Instruction],
        extra_signer: &Keypair,
    ) -> Result<(), BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        self.retry_executor
            .execute(|| self.send_tx_with_signer(instructions, extra_signer))
            .await
    }

    /// Send a transaction (internal, no rate limiting, payer signs only).
    async fn send_tx(&self, instructions: Vec<Instruction>) -> Result<(), BridgeError> {
        let recent_blockhash = self.rpc.get_latest_blockhash().await?;

        let tx = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &[self.payer.as_ref()],
            recent_blockhash,
        );

        self.rpc.send_and_confirm_transaction(&tx).await?;
        Ok(())
    }

    /// Send a transaction with operator as signer (internal, payer and operator sign).
    async fn send_tx_with_operator(&self, instructions: Vec<Instruction>) -> Result<(), BridgeError> {
        let recent_blockhash = self.rpc.get_latest_blockhash().await?;

        let tx = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &[self.payer.as_ref(), self.operator.as_ref()],
            recent_blockhash,
        );

        self.rpc.send_and_confirm_transaction(&tx).await?;
        Ok(())
    }

    /// Send a transaction with an additional signer (internal).
    async fn send_tx_with_signer(
        &self,
        instructions: &[Instruction],
        extra_signer: &Keypair,
    ) -> Result<(), BridgeError> {
        let recent_blockhash = self.rpc.get_latest_blockhash().await?;

        let tx = Transaction::new_signed_with_payer(
            instructions,
            Some(&self.payer.pubkey()),
            &[self.payer.as_ref(), extra_signer],
            recent_blockhash,
        );

        self.rpc.send_and_confirm_transaction(&tx).await?;
        Ok(())
    }
}
