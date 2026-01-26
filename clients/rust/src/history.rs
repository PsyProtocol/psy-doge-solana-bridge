//! Bridge history reconstruction client.
//!
//! This module provides efficient reconstruction of bridge history from Solana RPC,
//! allowing bridge nodes to rebuild their databases from on-chain state.
//!
//! Key features:
//! - Reconstruct TXO indices for each block from operator transactions
//! - Reconstruct pending mints for each block from operator transactions
//! - Stream full bridge interaction history
//! - Efficient batch fetching with rate limiting
//! - Checkpoint support for resumable syncs
//!
//! ## Buffer Reconstruction Strategy
//!
//! Buffer data (pending mints and TXO indices) is reconstructed by:
//! 1. Finding the operator address from block update transactions
//! 2. Finding the buffer accounts referenced in the block update
//! 3. Searching the operator's transaction history for buffer write instructions
//! 4. Parsing the instruction data to reconstruct the buffer contents
//!
//! This works because the operator must write to buffers before submitting a block update,
//! and the instruction data contains the actual buffer contents.

use std::collections::HashMap;
use std::sync::Arc;

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::UiTransactionEncoding;
use tokio::sync::mpsc;

use crate::config::RateLimitConfig;
use crate::errors::BridgeError;
use crate::rpc::RpcRateLimiter;
use crate::types::PendingMint;

use psy_doge_solana_core::data_accounts::pending_mint::{
    PendingMintsTxoBufferHeader, PendingMintsBufferStateHeader, PM_DA_PENDING_MINT_SIZE,
    PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE, PM_TXO_BUFFER_HEADER_SIZE,
};

// Used by the public fetch_*_from_buffer methods
#[allow(unused_imports)]
use psy_doge_solana_core::data_accounts::pending_mint as pm_data;
use psy_doge_solana_core::instructions::doge_bridge::{
    BlockUpdateFixedData,
    DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE, DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS,
    DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL, DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL,
    DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT,
};
use psy_doge_solana_core::program_state::FinalizedBlockMintTxoInfo;

/// Pending mint buffer instruction tag for insert operations
const PM_TAG_INSERT: u8 = 3;
/// TXO buffer instruction tag for write operations
const TXO_TAG_WRITE: u8 = 2;
/// TXO buffer instruction tag for set length operations
const TXO_TAG_SET_LEN: u8 = 1;

/// Cached operator transaction data for efficient buffer reconstruction.
///
/// Since the operator is static, we can fetch all operator transactions once
/// and reuse them when reconstructing buffer data for multiple blocks.
#[derive(Debug)]
struct OperatorTxCache {
    /// Parsed transaction data keyed by slot for efficient lookup
    transactions: Vec<ParsedOperatorTx>,
}

/// A parsed operator transaction with relevant buffer write data.
#[derive(Debug, Clone)]
struct ParsedOperatorTx {
    slot: u64,
    /// Is this a block update transaction?
    is_block_update: bool,
    /// Pending mint inserts: (buffer_account, group_idx, mints)
    mint_inserts: Vec<(Pubkey, u16, Vec<PendingMint>)>,
    /// TXO buffer set_len: (buffer_account, batch_id, size)
    txo_set_lens: Vec<(Pubkey, u32, u32)>,
    /// TXO buffer writes: (buffer_account, batch_id, offset, data)
    txo_writes: Vec<(Pubkey, u32, u32, Vec<u8>)>,
    /// Pending mint reinit: buffer_account (indicates new batch)
    mint_reinits: Vec<Pubkey>,
}

/// Block update info without buffer data (used during first pass).
#[derive(Debug)]
struct BlockUpdateInfo {
    block_height: u32,
    signature: Signature,
    slot: u64,
    block_time: Option<i64>,
    is_reorg: bool,
    extra_finalized_blocks: Vec<FinalizedBlockMintTxoInfo>,
    mint_buffer: Option<Pubkey>,
    txo_buffer: Option<Pubkey>,
    operator: Option<Pubkey>,
}

impl BlockUpdateInfo {
    fn to_block_record(self, pending_mints: Vec<PendingMint>, txo_indices: Vec<u32>) -> BlockRecord {
        BlockRecord {
            block_height: self.block_height,
            signature: self.signature,
            slot: self.slot,
            block_time: self.block_time,
            txo_indices,
            pending_mints,
            is_reorg: self.is_reorg,
            extra_finalized_blocks: self.extra_finalized_blocks,
        }
    }
}

/// A historical block record containing TXOs and pending mints.
#[derive(Debug, Clone)]
pub struct BlockRecord {
    /// Dogecoin block height
    pub block_height: u32,
    /// Solana transaction signature that finalized this block
    pub signature: Signature,
    /// Solana slot
    pub slot: u64,
    /// Block time (if available)
    pub block_time: Option<i64>,
    /// TXO indices for this block (spent outputs)
    pub txo_indices: Vec<u32>,
    /// Pending mints for this block (auto-claimed deposits)
    pub pending_mints: Vec<PendingMint>,
    /// Whether this was a reorg
    pub is_reorg: bool,
    /// Extra finalized blocks (for reorgs)
    pub extra_finalized_blocks: Vec<FinalizedBlockMintTxoInfo>,
}

/// A historical withdrawal request record.
#[derive(Debug, Clone)]
pub struct WithdrawalRequestRecord {
    /// Solana transaction signature
    pub signature: Signature,
    /// Solana slot
    pub slot: u64,
    /// Block time (if available)
    pub block_time: Option<i64>,
    /// Withdrawal amount in satoshis
    pub amount_sats: u64,
    /// Recipient Dogecoin address
    pub recipient_address: [u8; 20],
    /// Address type (0 = P2PKH, 1 = P2SH)
    pub address_type: u32,
    /// User's Solana pubkey
    pub user_pubkey: Pubkey,
}

/// A historical processed withdrawal record.
#[derive(Debug, Clone)]
pub struct ProcessedWithdrawalRecord {
    /// Solana transaction signature
    pub signature: Signature,
    /// Solana slot
    pub slot: u64,
    /// Block time (if available)
    pub block_time: Option<i64>,
    /// New return output sighash
    pub return_output_sighash: [u8; 32],
    /// New return output index
    pub return_output_index: u64,
    /// New return output amount
    pub return_output_amount: u64,
    /// New spent TXO tree root
    pub spent_txo_tree_root: [u8; 32],
    /// New next processed withdrawals index
    pub next_processed_withdrawals_index: u64,
}

/// A historical manual deposit claim record.
#[derive(Debug, Clone)]
pub struct ManualDepositRecord {
    /// Solana transaction signature
    pub signature: Signature,
    /// Solana slot
    pub slot: u64,
    /// Block time (if available)
    pub block_time: Option<i64>,
    /// Dogecoin transaction hash
    pub tx_hash: [u8; 32],
    /// Combined TXO index
    pub combined_txo_index: u64,
    /// Deposit amount in satoshis
    pub deposit_amount_sats: u64,
    /// Depositor's Solana public key
    pub depositor_pubkey: [u8; 32],
}

/// Unified history record type.
#[derive(Debug, Clone)]
pub enum HistoryRecord {
    /// Block transition (new finalized block)
    Block(BlockRecord),
    /// Withdrawal request
    WithdrawalRequest(WithdrawalRequestRecord),
    /// Processed withdrawal
    ProcessedWithdrawal(ProcessedWithdrawalRecord),
    /// Manual deposit claim
    ManualDeposit(ManualDepositRecord),
}

/// Checkpoint for resumable syncs.
#[derive(Debug, Clone)]
pub struct SyncCheckpoint {
    /// Last processed Solana signature
    pub last_signature: Signature,
    /// Last processed slot
    pub last_slot: u64,
    /// Number of records processed
    pub records_processed: u64,
    /// Last processed Dogecoin block height (if any)
    pub last_block_height: Option<u32>,
}

impl SyncCheckpoint {
    /// Create a new empty checkpoint.
    pub fn new() -> Self {
        Self {
            last_signature: Signature::default(),
            last_slot: 0,
            records_processed: 0,
            last_block_height: None,
        }
    }
}

impl Default for SyncCheckpoint {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for the history sync client.
#[derive(Debug, Clone)]
pub struct HistorySyncConfig {
    /// Solana RPC URL
    pub rpc_url: String,
    /// Bridge program ID
    pub program_id: Pubkey,
    /// Bridge state PDA
    pub bridge_state_pda: Pubkey,
    /// Pending mint buffer program ID
    pub pending_mint_program_id: Pubkey,
    /// TXO buffer program ID
    pub txo_buffer_program_id: Pubkey,
    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,
    /// Batch size for signature fetches
    pub signature_batch_size: usize,
    /// Whether to include withdrawal requests
    pub include_withdrawals: bool,
    /// Whether to include manual deposits
    pub include_manual_deposits: bool,
}

impl HistorySyncConfig {
    /// Create a new config with required parameters.
    pub fn new(
        rpc_url: impl Into<String>,
        program_id: Pubkey,
        bridge_state_pda: Pubkey,
        pending_mint_program_id: Pubkey,
        txo_buffer_program_id: Pubkey,
    ) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            program_id,
            bridge_state_pda,
            pending_mint_program_id,
            txo_buffer_program_id,
            rate_limit: RateLimitConfig::default(),
            signature_batch_size: 100,
            include_withdrawals: true,
            include_manual_deposits: true,
        }
    }

    /// Set the rate limit configuration.
    pub fn rate_limit(mut self, config: RateLimitConfig) -> Self {
        self.rate_limit = config;
        self
    }

    /// Set the signature batch size.
    pub fn signature_batch_size(mut self, size: usize) -> Self {
        self.signature_batch_size = size;
        self
    }

    /// Set whether to include withdrawal requests.
    pub fn include_withdrawals(mut self, include: bool) -> Self {
        self.include_withdrawals = include;
        self
    }

    /// Set whether to include manual deposits.
    pub fn include_manual_deposits(mut self, include: bool) -> Self {
        self.include_manual_deposits = include;
        self
    }
}

/// Bridge history sync client for reconstructing state from Solana.
///
/// This client efficiently fetches and reconstructs bridge history,
/// allowing bridge nodes to rebuild their databases from on-chain state.
///
/// # Features
///
/// - **Block reconstruction**: Extract TXO indices and pending mints for each block
/// - **Streaming API**: Process records as they're fetched
/// - **Rate limiting**: Avoid hitting RPC limits
/// - **Checkpointing**: Resume syncs from where you left off
///
/// # Example
///
/// ```ignore
/// use doge_bridge_client::history::{BridgeHistorySync, HistorySyncConfig};
///
/// let config = HistorySyncConfig::new(
///     "https://api.mainnet-beta.solana.com",
///     program_id,
///     bridge_state_pda,
///     pending_mint_program_id,
///     txo_buffer_program_id,
/// );
///
/// let sync = BridgeHistorySync::new(config)?;
///
/// // Stream all history
/// let mut receiver = sync.stream_history(None).await?;
/// while let Some(record) = receiver.recv().await {
///     match record {
///         HistoryRecord::Block(block) => {
///             println!("Block {}: {} mints, {} txos",
///                 block.block_height,
///                 block.pending_mints.len(),
///                 block.txo_indices.len());
///         }
///         _ => {}
///     }
/// }
/// ```
pub struct BridgeHistorySync {
    config: HistorySyncConfig,
    rpc: Arc<RpcClient>,
    rate_limiter: Arc<RpcRateLimiter>,
}

impl BridgeHistorySync {
    /// Create a new history sync client.
    pub fn new(config: HistorySyncConfig) -> Result<Self, BridgeError> {
        let rpc = Arc::new(RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        ));
        let rate_limiter = Arc::new(RpcRateLimiter::new(config.rate_limit.clone()));

        Ok(Self {
            config,
            rpc,
            rate_limiter,
        })
    }

    /// Stream bridge history starting from a checkpoint.
    ///
    /// Returns a receiver that will receive all history records in chronological order.
    pub async fn stream_history(
        &self,
        checkpoint: Option<SyncCheckpoint>,
    ) -> Result<(mpsc::Receiver<HistoryRecord>, SyncHandle), BridgeError> {
        let (sender, receiver) = mpsc::channel(1000);
        let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel();
        let (checkpoint_sender, checkpoint_receiver) = mpsc::channel(1);

        let rpc = self.rpc.clone();
        let rate_limiter = self.rate_limiter.clone();
        let config = self.config.clone();
        let start_checkpoint = checkpoint.unwrap_or_default();

        let handle = tokio::spawn(async move {
            Self::stream_loop(
                rpc,
                rate_limiter,
                config,
                sender,
                checkpoint_sender,
                start_checkpoint,
                stop_receiver,
            )
            .await
        });

        Ok((
            receiver,
            SyncHandle {
                stop_sender: Some(stop_sender),
                checkpoint_receiver,
                handle: Some(handle),
            },
        ))
    }

    /// Fetch all block records in a range.
    ///
    /// This method is optimized for efficiency:
    /// 1. First pass: collect all block update transactions and identify the operator
    /// 2. Fetch operator transactions once (operator is static)
    /// 3. Second pass: reconstruct buffer data for each block using cached transactions
    pub async fn fetch_blocks(
        &self,
        from_height: Option<u32>,
        to_height: Option<u32>,
    ) -> Result<Vec<BlockRecord>, BridgeError> {
        // First pass: collect block update info without buffer reconstruction
        let mut block_infos: Vec<BlockUpdateInfo> = Vec::new();
        let mut operator: Option<Pubkey> = None;
        let mut before_sig: Option<Signature> = None;

        loop {
            let _guard = self.rate_limiter.acquire().await?;

            let signatures = self
                .rpc
                .get_signatures_for_address_with_config(
                    &self.config.bridge_state_pda,
                    solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                        before: before_sig,
                        until: None,
                        limit: Some(self.config.signature_batch_size),
                        commitment: Some(CommitmentConfig::confirmed()),
                    },
                )
                .await?;

            if signatures.is_empty() {
                break;
            }

            for sig_info in &signatures {
                if let Ok(sig) = sig_info.signature.parse::<Signature>() {
                    if let Some(info) = self.fetch_block_update_info(&sig).await? {
                        // Filter by height range
                        if let Some(from) = from_height {
                            if info.block_height < from {
                                before_sig = Some(sig);
                                continue;
                            }
                        }
                        if let Some(to) = to_height {
                            if info.block_height > to {
                                // We've passed the range, but keep going in case of reorgs
                            }
                        }
                        // Capture operator from first block update
                        if operator.is_none() {
                            operator = info.operator;
                        }
                        block_infos.push(info);
                    }
                    before_sig = Some(sig);
                }
            }
        }

        // If no blocks found or no operator, return empty
        if block_infos.is_empty() {
            return Ok(Vec::new());
        }

        let operator = match operator {
            Some(op) => op,
            None => return Ok(block_infos.into_iter().map(|info| info.to_block_record(Vec::new(), Vec::new())).collect()),
        };

        // Second pass: fetch operator transactions once and build cache
        let op_tx_cache = self.build_operator_tx_cache(&operator).await?;

        // Third pass: reconstruct buffer data for each block
        let mut blocks = Vec::new();
        for info in block_infos {
            let (pending_mints, txo_indices) = Self::reconstruct_buffers_from_cache(
                &op_tx_cache,
                &info.mint_buffer.unwrap_or(Pubkey::default()),
                &info.txo_buffer.unwrap_or(Pubkey::default()),
                info.slot,
            );
            blocks.push(info.to_block_record(pending_mints, txo_indices));
        }

        // Sort by block height
        blocks.sort_by_key(|b| b.block_height);
        Ok(blocks)
    }

    /// Build a cache of operator transactions for efficient buffer reconstruction.
    async fn build_operator_tx_cache(&self, operator: &Pubkey) -> Result<OperatorTxCache, BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let signatures = self
            .rpc
            .get_signatures_for_address_with_config(
                operator,
                solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                    before: None,
                    until: None,
                    limit: Some(1000), // Get more transactions for the cache
                    commitment: Some(CommitmentConfig::confirmed()),
                },
            )
            .await?;

        let mut transactions = Vec::new();

        for sig_info in signatures {
            let sig = match sig_info.signature.parse::<Signature>() {
                Ok(s) => s,
                Err(_) => continue,
            };

            let _guard = self.rate_limiter.acquire().await?;

            let tx = match self
                .rpc
                .get_transaction_with_config(
                    &sig,
                    RpcTransactionConfig {
                        encoding: Some(UiTransactionEncoding::Base64),
                        commitment: Some(CommitmentConfig::confirmed()),
                        max_supported_transaction_version: Some(0),
                    },
                )
                .await
            {
                Ok(tx) => tx,
                Err(_) => continue,
            };

            let transaction = match tx.transaction.transaction {
                solana_transaction_status::EncodedTransaction::Binary(data, _) => {
                    use base64::Engine;
                    let bytes = match base64::engine::general_purpose::STANDARD.decode(&data) {
                        Ok(b) => b,
                        Err(_) => continue,
                    };
                    match bincode::deserialize::<solana_sdk::transaction::VersionedTransaction>(&bytes) {
                        Ok(tx) => tx,
                        Err(_) => continue,
                    }
                }
                _ => continue,
            };

            let parsed = self.parse_operator_tx(sig_info.slot, &transaction);
            transactions.push(parsed);
        }

        Ok(OperatorTxCache { transactions })
    }

    /// Parse an operator transaction into a structured format.
    fn parse_operator_tx(&self, slot: u64, transaction: &solana_sdk::transaction::VersionedTransaction) -> ParsedOperatorTx {
        let message = &transaction.message;
        let account_keys = message.static_account_keys();

        let mut parsed = ParsedOperatorTx {
            slot,
            is_block_update: false,
            mint_inserts: Vec::new(),
            txo_set_lens: Vec::new(),
            txo_writes: Vec::new(),
            mint_reinits: Vec::new(),
        };

        for ix in message.instructions() {
            let program_idx = ix.program_id_index as usize;
            if program_idx >= account_keys.len() {
                continue;
            }

            let program_id = account_keys[program_idx];

            // Check for block update
            if program_id == self.config.program_id && ix.data.len() >= 5 {
                let discriminator = ix.data[4];
                if discriminator == DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE ||
                   discriminator == DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS {
                    parsed.is_block_update = true;
                }
            }

            // Check for pending mint buffer instructions
            if program_id == self.config.pending_mint_program_id {
                // Get buffer account
                if ix.accounts.is_empty() {
                    continue;
                }
                let buffer_idx = ix.accounts[0] as usize;
                if buffer_idx >= account_keys.len() {
                    continue;
                }
                let buffer = account_keys[buffer_idx];

                // Reinit (tag 1)
                if !ix.data.is_empty() && ix.data[0] == 1 {
                    parsed.mint_reinits.push(buffer);
                }

                // Insert (tag 3)
                if ix.data.len() >= 3 && ix.data[0] == PM_TAG_INSERT {
                    let group_idx = u16::from_le_bytes([ix.data[1], ix.data[2]]);
                    let mint_data = &ix.data[3..];

                    let mut mints = Vec::new();
                    for chunk in mint_data.chunks_exact(PM_DA_PENDING_MINT_SIZE) {
                        if chunk.len() >= 40 {
                            let mut recipient = [0u8; 32];
                            recipient.copy_from_slice(&chunk[0..32]);
                            if let Ok(amount_bytes) = chunk[32..40].try_into() {
                                let amount = u64::from_le_bytes(amount_bytes);
                                mints.push(PendingMint { recipient, amount });
                            }
                        }
                    }

                    if !mints.is_empty() {
                        parsed.mint_inserts.push((buffer, group_idx, mints));
                    }
                }
            }

            // Check for TXO buffer instructions
            if program_id == self.config.txo_buffer_program_id {
                // Get buffer account
                if ix.accounts.is_empty() {
                    continue;
                }
                let buffer_idx = ix.accounts[0] as usize;
                if buffer_idx >= account_keys.len() {
                    continue;
                }
                let buffer = account_keys[buffer_idx];

                // SetLen (tag 1)
                if ix.data.len() >= 14 && ix.data[0] == TXO_TAG_SET_LEN {
                    let new_len = u32::from_le_bytes([ix.data[1], ix.data[2], ix.data[3], ix.data[4]]);
                    let batch_id = u32::from_le_bytes([ix.data[6], ix.data[7], ix.data[8], ix.data[9]]);
                    parsed.txo_set_lens.push((buffer, batch_id, new_len));
                }

                // Write (tag 2)
                if ix.data.len() >= 9 && ix.data[0] == TXO_TAG_WRITE {
                    let batch_id = u32::from_le_bytes([ix.data[1], ix.data[2], ix.data[3], ix.data[4]]);
                    let offset = u32::from_le_bytes([ix.data[5], ix.data[6], ix.data[7], ix.data[8]]);
                    let data = ix.data[9..].to_vec();
                    parsed.txo_writes.push((buffer, batch_id, offset, data));
                }
            }
        }

        parsed
    }

    /// Reconstruct buffer data from the cached operator transactions.
    fn reconstruct_buffers_from_cache(
        cache: &OperatorTxCache,
        mint_buffer: &Pubkey,
        txo_buffer: &Pubkey,
        block_update_slot: u64,
    ) -> (Vec<PendingMint>, Vec<u32>) {
        // Filter transactions up to and including the block update slot
        // Transactions in cache are in reverse chronological order (newest first)
        let relevant_txs: Vec<_> = cache.transactions.iter()
            .filter(|tx| tx.slot <= block_update_slot)
            .collect();

        let mut pending_mints: HashMap<u16, Vec<PendingMint>> = HashMap::new();
        let mut txo_writes: HashMap<u32, Vec<u8>> = HashMap::new();
        let mut txo_data_size: u32 = 0;
        let mut txo_batch_id: Option<u32> = None;
        let mut found_block_update = false;

        for tx in relevant_txs {
            // If we've found our block update and hit another one, stop
            if tx.is_block_update {
                if found_block_update {
                    break;
                }
                found_block_update = true;
            }

            // Process mint reinits
            for buffer in &tx.mint_reinits {
                if buffer == mint_buffer {
                    if found_block_update {
                        // Past our batch
                        break;
                    }
                    pending_mints.clear();
                }
            }

            // Process mint inserts
            for (buffer, group_idx, mints) in &tx.mint_inserts {
                if buffer == mint_buffer {
                    pending_mints.insert(*group_idx, mints.clone());
                }
            }

            // Process TXO set_len
            for (buffer, batch_id, size) in &tx.txo_set_lens {
                if buffer == txo_buffer {
                    match txo_batch_id {
                        None => {
                            txo_batch_id = Some(*batch_id);
                            txo_data_size = *size;
                        }
                        Some(current) if current == *batch_id => {
                            txo_data_size = *size;
                        }
                        Some(_) => {
                            if found_block_update {
                                break;
                            }
                            txo_writes.clear();
                            txo_batch_id = Some(*batch_id);
                            txo_data_size = *size;
                        }
                    }
                }
            }

            // Process TXO writes
            for (buffer, batch_id, offset, data) in &tx.txo_writes {
                if buffer == txo_buffer {
                    if txo_batch_id.is_none() || txo_batch_id == Some(*batch_id) {
                        txo_batch_id = Some(*batch_id);
                        txo_writes.insert(*offset, data.clone());
                    }
                }
            }
        }

        // Assemble pending mints in order
        let mut all_mints = Vec::new();
        let mut group_indices: Vec<_> = pending_mints.keys().cloned().collect();
        group_indices.sort();
        for idx in group_indices {
            if let Some(mints) = pending_mints.remove(&idx) {
                all_mints.extend(mints);
            }
        }

        // Assemble TXO data from writes
        let txo_indices = Self::assemble_txo_indices(&txo_writes, txo_data_size);

        (all_mints, txo_indices)
    }

    /// Fetch block update info without reconstructing buffer data.
    async fn fetch_block_update_info(&self, signature: &Signature) -> Result<Option<BlockUpdateInfo>, BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let tx = self
            .rpc
            .get_transaction_with_config(
                signature,
                RpcTransactionConfig {
                    encoding: Some(UiTransactionEncoding::Base64),
                    commitment: Some(CommitmentConfig::confirmed()),
                    max_supported_transaction_version: Some(0),
                },
            )
            .await?;

        let slot = tx.slot;
        let block_time = tx.block_time;

        let transaction = match tx.transaction.transaction {
            solana_transaction_status::EncodedTransaction::Binary(data, _) => {
                use base64::Engine;
                let bytes = match base64::engine::general_purpose::STANDARD.decode(&data) {
                    Ok(b) => b,
                    Err(_) => return Ok(None),
                };
                match bincode::deserialize::<solana_sdk::transaction::VersionedTransaction>(&bytes) {
                    Ok(tx) => tx,
                    Err(_) => return Ok(None),
                }
            }
            _ => return Ok(None),
        };

        let message = transaction.message;
        let account_keys = message.static_account_keys();

        for ix in message.instructions() {
            let program_idx = ix.program_id_index as usize;
            if program_idx >= account_keys.len() {
                continue;
            }

            let program_id = account_keys[program_idx];

            // Only process our bridge program
            if program_id != self.config.program_id {
                continue;
            }

            if ix.data.len() < 5 {
                continue;
            }

            let discriminator = ix.data[4];

            if discriminator == DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE ||
               discriminator == DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS {
                let is_reorg = discriminator == DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS;

                // Parse block height from instruction data
                let data_offset = 8;
                let fixed_size = std::mem::size_of::<BlockUpdateFixedData>();
                if ix.data.len() < data_offset + fixed_size {
                    return Ok(None);
                }

                let fixed: &BlockUpdateFixedData =
                    bytemuck::from_bytes(&ix.data[data_offset..data_offset + fixed_size]);

                let block_height = fixed.header.finalized_state.block_height;

                // Parse extra blocks for reorgs
                let mut extra_blocks = Vec::new();
                if is_reorg {
                    let item_size = std::mem::size_of::<FinalizedBlockMintTxoInfo>();
                    let remaining = &ix.data[data_offset + fixed_size..];
                    for chunk in remaining.chunks_exact(item_size) {
                        let item: &FinalizedBlockMintTxoInfo = bytemuck::from_bytes(chunk);
                        extra_blocks.push(*item);
                    }
                }

                // Get accounts
                let mint_buffer = if ix.accounts.len() > 1 && (ix.accounts[1] as usize) < account_keys.len() {
                    Some(account_keys[ix.accounts[1] as usize])
                } else {
                    None
                };

                let txo_buffer = if ix.accounts.len() > 2 && (ix.accounts[2] as usize) < account_keys.len() {
                    Some(account_keys[ix.accounts[2] as usize])
                } else {
                    None
                };

                let operator = if ix.accounts.len() > 3 && (ix.accounts[3] as usize) < account_keys.len() {
                    Some(account_keys[ix.accounts[3] as usize])
                } else {
                    None
                };

                return Ok(Some(BlockUpdateInfo {
                    block_height,
                    signature: *signature,
                    slot,
                    block_time,
                    is_reorg,
                    extra_finalized_blocks: extra_blocks,
                    mint_buffer,
                    txo_buffer,
                    operator,
                }));
            }
        }

        Ok(None)
    }

    /// Reconstruct TXO indices and pending mints for a specific block height.
    pub async fn reconstruct_block(&self, block_height: u32) -> Result<Option<BlockRecord>, BridgeError> {
        let blocks = self.fetch_blocks(Some(block_height), Some(block_height)).await?;
        Ok(blocks.into_iter().find(|b| b.block_height == block_height))
    }

    /// Fetch pending mints from a mint buffer account.
    pub async fn fetch_pending_mints_from_buffer(
        &self,
        buffer_account: Pubkey,
    ) -> Result<Vec<PendingMint>, BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let account = self.rpc.get_account(&buffer_account).await?;
        let data = account.data;

        if data.len() < PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE {
            return Err(BridgeError::InvalidInput("Buffer too small".into()));
        }

        let header: &PendingMintsBufferStateHeader =
            bytemuck::from_bytes(&data[..PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE]);

        let mint_count = header.pending_mints_count as usize;
        let groups_count = header.pending_mint_groups_count as usize;
        let mints_offset = PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE + (groups_count * 32);

        let mut mints = Vec::with_capacity(mint_count);
        for i in 0..mint_count {
            let offset = mints_offset + (i * PM_DA_PENDING_MINT_SIZE);
            if offset + PM_DA_PENDING_MINT_SIZE > data.len() {
                break;
            }
            let mint: &PendingMint = bytemuck::from_bytes(&data[offset..offset + PM_DA_PENDING_MINT_SIZE]);
            mints.push(*mint);
        }

        Ok(mints)
    }

    /// Fetch TXO indices from a TXO buffer account.
    pub async fn fetch_txos_from_buffer(
        &self,
        buffer_account: Pubkey,
    ) -> Result<Vec<u32>, BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        let account = self.rpc.get_account(&buffer_account).await?;
        let data = account.data;

        if data.len() < PM_TXO_BUFFER_HEADER_SIZE {
            return Err(BridgeError::InvalidInput("Buffer too small".into()));
        }

        let header: &PendingMintsTxoBufferHeader =
            bytemuck::from_bytes(&data[..PM_TXO_BUFFER_HEADER_SIZE]);

        let data_size = header.data_size as usize;
        let txo_count = data_size / 4;

        let mut txos = Vec::with_capacity(txo_count);
        for i in 0..txo_count {
            let offset = PM_TXO_BUFFER_HEADER_SIZE + (i * 4);
            if offset + 4 > data.len() {
                break;
            }
            let txo = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
            txos.push(txo);
        }

        Ok(txos)
    }

    /// Internal streaming loop.
    async fn stream_loop(
        rpc: Arc<RpcClient>,
        rate_limiter: Arc<RpcRateLimiter>,
        config: HistorySyncConfig,
        sender: mpsc::Sender<HistoryRecord>,
        checkpoint_sender: mpsc::Sender<SyncCheckpoint>,
        mut checkpoint: SyncCheckpoint,
        mut stop_receiver: tokio::sync::oneshot::Receiver<()>,
    ) {
        let after_sig = if checkpoint.last_signature == Signature::default() {
            None
        } else {
            Some(checkpoint.last_signature)
        };

        // Collect all signatures first (oldest to newest)
        let signatures = match Self::collect_all_signatures(
            &rpc,
            &rate_limiter,
            &config.bridge_state_pda,
            after_sig,
            config.signature_batch_size,
        )
        .await
        {
            Ok(sigs) => sigs,
            Err(e) => {
                tracing::error!("Failed to collect signatures: {}", e);
                return;
            }
        };

        // Process in chronological order (oldest first)
        for sig in signatures.into_iter().rev() {
            // Check for stop signal
            if stop_receiver.try_recv().is_ok() {
                break;
            }

            match Self::process_transaction(
                &rpc,
                &rate_limiter,
                &config,
                &sig,
            )
            .await
            {
                Ok(Some(record)) => {
                    // Update checkpoint
                    if let HistoryRecord::Block(ref block) = record {
                        checkpoint.last_block_height = Some(block.block_height);
                    }
                    checkpoint.last_signature = sig;
                    checkpoint.records_processed += 1;

                    // Send record
                    if sender.send(record).await.is_err() {
                        break;
                    }

                    // Send checkpoint update
                    let _ = checkpoint_sender.try_send(checkpoint.clone());
                }
                Ok(None) => {
                    // Update checkpoint even for non-matching transactions
                    checkpoint.last_signature = sig;
                }
                Err(e) => {
                    tracing::warn!("Error processing transaction {}: {}", sig, e);
                }
            }
        }
    }

    /// Collect all signatures for an address.
    async fn collect_all_signatures(
        rpc: &RpcClient,
        rate_limiter: &RpcRateLimiter,
        address: &Pubkey,
        after_signature: Option<Signature>,
        batch_size: usize,
    ) -> Result<Vec<Signature>, BridgeError> {
        let mut all_signatures = Vec::new();
        let mut before_sig: Option<Signature> = None;

        loop {
            let _guard = rate_limiter.acquire().await?;

            let signatures = rpc
                .get_signatures_for_address_with_config(
                    address,
                    solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                        before: before_sig,
                        until: after_signature,
                        limit: Some(batch_size),
                        commitment: Some(CommitmentConfig::confirmed()),
                    },
                )
                .await?;

            if signatures.is_empty() {
                break;
            }

            for sig_info in &signatures {
                if let Ok(sig) = sig_info.signature.parse::<Signature>() {
                    all_signatures.push(sig);
                    before_sig = Some(sig);
                }
            }
        }

        Ok(all_signatures)
    }

    /// Process a single transaction.
    #[allow(unused_variables)]
    async fn process_transaction(
        rpc: &RpcClient,
        rate_limiter: &RpcRateLimiter,
        config: &HistorySyncConfig,
        signature: &Signature,
    ) -> Result<Option<HistoryRecord>, BridgeError> {
        let _guard = rate_limiter.acquire().await?;

        let tx = rpc
            .get_transaction_with_config(
                signature,
                RpcTransactionConfig {
                    encoding: Some(UiTransactionEncoding::Base64),
                    commitment: Some(CommitmentConfig::confirmed()),
                    max_supported_transaction_version: Some(0),
                },
            )
            .await?;

        let slot = tx.slot;
        let block_time = tx.block_time;

        // Extract the transaction data
        let transaction = match tx.transaction.transaction {
            solana_transaction_status::EncodedTransaction::Binary(data, _) => {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD.decode(&data)
                    .map_err(|e| BridgeError::InvalidInput(format!("Failed to decode tx: {}", e)))?;
                bincode::deserialize::<solana_sdk::transaction::VersionedTransaction>(&bytes)
                    .map_err(|e| BridgeError::InvalidInput(format!("Failed to deserialize tx: {}", e)))?
            }
            _ => return Ok(None),
        };

        // Find instructions to this program
        let message = transaction.message;
        let account_keys = message.static_account_keys();

        for ix in message.instructions() {
            let program_idx = ix.program_id_index as usize;
            if program_idx >= account_keys.len() {
                continue;
            }

            if account_keys[program_idx] != config.program_id {
                continue;
            }

            let ix_data = &ix.data;
            if ix_data.len() < 5 {
                continue;
            }

            let discriminator = ix_data[4];

            match discriminator {
                DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE => {
                    return Self::parse_block_update(
                        rpc,
                        rate_limiter,
                        config,
                        signature,
                        slot,
                        block_time,
                        ix_data,
                        &ix.accounts,
                        account_keys,
                        false,
                    )
                    .await;
                }
                DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS => {
                    return Self::parse_block_update(
                        rpc,
                        rate_limiter,
                        config,
                        signature,
                        slot,
                        block_time,
                        ix_data,
                        &ix.accounts,
                        account_keys,
                        true,
                    )
                    .await;
                }
                DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL if config.include_withdrawals => {
                    return Self::parse_withdrawal_request(
                        signature,
                        slot,
                        block_time,
                        ix_data,
                        &ix.accounts,
                        account_keys,
                    );
                }
                DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL if config.include_withdrawals => {
                    return Self::parse_processed_withdrawal(
                        signature,
                        slot,
                        block_time,
                        ix_data,
                    );
                }
                DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT if config.include_manual_deposits => {
                    return Self::parse_manual_deposit(
                        signature,
                        slot,
                        block_time,
                        ix_data,
                        &ix.accounts,
                        account_keys,
                    );
                }
                _ => {}
            }
        }

        Ok(None)
    }

    /// Parse a block update transaction and reconstruct buffer data from operator transactions.
    async fn parse_block_update(
        rpc: &RpcClient,
        rate_limiter: &RpcRateLimiter,
        config: &HistorySyncConfig,
        signature: &Signature,
        slot: u64,
        block_time: Option<i64>,
        ix_data: &[u8],
        accounts: &[u8],
        account_keys: &[Pubkey],
        is_reorg: bool,
    ) -> Result<Option<HistoryRecord>, BridgeError> {
        // Skip 8-byte alignment header (includes bumps at end)
        let data_offset = 8;

        // Parse header to get block height
        let fixed_size = std::mem::size_of::<BlockUpdateFixedData>();
        if ix_data.len() < data_offset + fixed_size {
            return Ok(None);
        }

        let fixed: &BlockUpdateFixedData =
            bytemuck::from_bytes(&ix_data[data_offset..data_offset + fixed_size]);

        let block_height = fixed.header.finalized_state.block_height;

        // Parse extra blocks for reorgs
        let mut extra_blocks = Vec::new();
        if is_reorg {
            let item_size = std::mem::size_of::<FinalizedBlockMintTxoInfo>();
            let remaining = &ix_data[data_offset + fixed_size..];
            for chunk in remaining.chunks_exact(item_size) {
                let item: &FinalizedBlockMintTxoInfo = bytemuck::from_bytes(chunk);
                extra_blocks.push(*item);
            }
        }

        // Get accounts from instruction account indices
        // Account layout from block_update(): [bridge_state, mint_buffer, txo_buffer, operator, payer, ...]
        let mint_buffer = if accounts.len() > 1 && (accounts[1] as usize) < account_keys.len() {
            Some(account_keys[accounts[1] as usize])
        } else {
            None
        };

        let txo_buffer = if accounts.len() > 2 && (accounts[2] as usize) < account_keys.len() {
            Some(account_keys[accounts[2] as usize])
        } else {
            None
        };

        let operator = if accounts.len() > 3 && (accounts[3] as usize) < account_keys.len() {
            Some(account_keys[accounts[3] as usize])
        } else {
            None
        };

        // Reconstruct buffer data from operator's transaction history
        let (pending_mints, txo_indices) = if let (Some(op), Some(mint_buf), Some(txo_buf)) = (operator, mint_buffer, txo_buffer) {
            Self::reconstruct_buffers_from_operator_txs(
                rpc,
                rate_limiter,
                config,
                &op,
                &mint_buf,
                &txo_buf,
                slot,
            ).await.unwrap_or((Vec::new(), Vec::new()))
        } else {
            (Vec::new(), Vec::new())
        };

        Ok(Some(HistoryRecord::Block(BlockRecord {
            block_height,
            signature: *signature,
            slot,
            block_time,
            txo_indices,
            pending_mints,
            is_reorg,
            extra_finalized_blocks: extra_blocks,
        })))
    }

    /// Reconstruct buffer data by finding operator's buffer write transactions.
    ///
    /// This method searches the operator's transaction history to find the buffer write
    /// instructions that populated the mint and TXO buffers before the block update.
    ///
    /// The key insight is that buffers are reinitialized before each block, so we need
    /// to find the most recent batch of writes. We detect batch boundaries by:
    /// - For TXO buffer: The `set_len` instruction starts a new batch
    /// - For pending mints: The `reinit` instruction (tag 1) starts a new batch
    ///
    /// We process transactions in reverse chronological order and stop when we find
    /// a complete set of writes for the most recent batch.
    async fn reconstruct_buffers_from_operator_txs(
        rpc: &RpcClient,
        rate_limiter: &RpcRateLimiter,
        config: &HistorySyncConfig,
        operator: &Pubkey,
        mint_buffer: &Pubkey,
        txo_buffer: &Pubkey,
        block_update_slot: u64,
    ) -> Result<(Vec<PendingMint>, Vec<u32>), BridgeError> {
        // Fetch recent transactions from the operator before the block update slot
        let _guard = rate_limiter.acquire().await?;

        let signatures = rpc
            .get_signatures_for_address_with_config(
                operator,
                solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                    before: None,
                    until: None,
                    limit: Some(100), // Look at recent transactions
                    commitment: Some(CommitmentConfig::confirmed()),
                },
            )
            .await?;

        // Filter to transactions in slots up to and including the block update
        // Transactions are returned in reverse chronological order (newest first)
        let relevant_sigs: Vec<_> = signatures
            .into_iter()
            .filter(|s| s.slot <= block_update_slot)
            .collect();

        // We want to process in chronological order to find the most recent batch
        // But first, let's find a cutoff - we stop when we hit a previous block_update
        // or when we've collected enough data

        // Track current batch data
        let mut pending_mints: HashMap<u16, Vec<PendingMint>> = HashMap::new();
        let mut txo_writes: HashMap<u32, Vec<u8>> = HashMap::new();
        let mut txo_data_size: u32 = 0;
        let mut txo_batch_id: Option<u32> = None;
        let mut found_block_update_in_batch = false;

        // Process transactions from newest to oldest
        // When we hit a reinit/set_len for a different batch, we've gone too far back
        for sig_info in relevant_sigs.iter() {
            let sig = match sig_info.signature.parse::<Signature>() {
                Ok(s) => s,
                Err(_) => continue,
            };

            let _guard = rate_limiter.acquire().await?;

            let tx = match rpc
                .get_transaction_with_config(
                    &sig,
                    RpcTransactionConfig {
                        encoding: Some(UiTransactionEncoding::Base64),
                        commitment: Some(CommitmentConfig::confirmed()),
                        max_supported_transaction_version: Some(0),
                    },
                )
                .await
            {
                Ok(tx) => tx,
                Err(_) => continue,
            };

            let transaction = match tx.transaction.transaction {
                solana_transaction_status::EncodedTransaction::Binary(data, _) => {
                    use base64::Engine;
                    let bytes = match base64::engine::general_purpose::STANDARD.decode(&data) {
                        Ok(b) => b,
                        Err(_) => continue,
                    };
                    match bincode::deserialize::<solana_sdk::transaction::VersionedTransaction>(&bytes) {
                        Ok(tx) => tx,
                        Err(_) => continue,
                    }
                }
                _ => continue,
            };

            let message = transaction.message;
            let account_keys = message.static_account_keys();

            // Check if this transaction contains a block_update to our bridge
            let mut is_block_update_tx = false;
            for ix in message.instructions() {
                let program_idx = ix.program_id_index as usize;
                if program_idx >= account_keys.len() {
                    continue;
                }
                if account_keys[program_idx] == config.program_id && ix.data.len() >= 5 {
                    let discriminator = ix.data[4];
                    if discriminator == DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE ||
                       discriminator == DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS {
                        is_block_update_tx = true;
                        break;
                    }
                }
            }

            // If we've already found our block update and hit another one, stop
            if is_block_update_tx {
                if found_block_update_in_batch {
                    // We've gone back to a previous block's data, stop
                    break;
                }
                found_block_update_in_batch = true;
            }

            // Process buffer writes
            for ix in message.instructions() {
                let program_idx = ix.program_id_index as usize;
                if program_idx >= account_keys.len() {
                    continue;
                }

                let program_id = account_keys[program_idx];

                // Check for pending mint buffer instructions
                if program_id == config.pending_mint_program_id {
                    // Check for reinit (tag 1) which starts a new batch
                    if !ix.data.is_empty() && ix.data[0] == 1 {
                        // This is a reinit - if we haven't found our block update yet,
                        // this might be setting up for our block. If we have, stop.
                        if found_block_update_in_batch {
                            // We've gone past our batch
                            break;
                        }
                        // Clear any existing data - new batch starting
                        pending_mints.clear();
                    }

                    if let Some((group_idx, mints)) = Self::parse_pending_mint_insert(&ix.data, &ix.accounts, account_keys, mint_buffer) {
                        pending_mints.insert(group_idx, mints);
                    }
                }

                // Check for TXO buffer instructions
                if program_id == config.txo_buffer_program_id {
                    if let Some((batch_id, size)) = Self::parse_txo_buffer_set_len_with_batch(&ix.data, &ix.accounts, account_keys, txo_buffer) {
                        match txo_batch_id {
                            None => {
                                // First batch we've seen - use it
                                txo_batch_id = Some(batch_id);
                                txo_data_size = size;
                            }
                            Some(current) if current == batch_id => {
                                // Same batch, update size
                                txo_data_size = size;
                            }
                            Some(_) => {
                                // Different batch - if we've found our block update, we've gone too far
                                if found_block_update_in_batch {
                                    break;
                                }
                                // Otherwise, this is a newer batch, clear old data
                                txo_writes.clear();
                                txo_batch_id = Some(batch_id);
                                txo_data_size = size;
                            }
                        }
                    }

                    if let Some((batch_id, offset, data)) = Self::parse_txo_buffer_write_with_batch(&ix.data, &ix.accounts, account_keys, txo_buffer) {
                        // Only use writes from the current batch
                        if txo_batch_id.is_none() || txo_batch_id == Some(batch_id) {
                            txo_batch_id = Some(batch_id);
                            txo_writes.insert(offset, data);
                        }
                    }
                }
            }
        }

        // Assemble pending mints in order
        let mut all_mints = Vec::new();
        let mut group_indices: Vec<_> = pending_mints.keys().cloned().collect();
        group_indices.sort();
        for idx in group_indices {
            if let Some(mints) = pending_mints.remove(&idx) {
                all_mints.extend(mints);
            }
        }

        // Assemble TXO data from writes
        let txo_indices = Self::assemble_txo_indices(&txo_writes, txo_data_size);

        Ok((all_mints, txo_indices))
    }

    /// Parse a txo_buffer_set_len instruction and return batch_id and size.
    fn parse_txo_buffer_set_len_with_batch(
        ix_data: &[u8],
        accounts: &[u8],
        account_keys: &[Pubkey],
        expected_buffer: &Pubkey,
    ) -> Option<(u32, u32)> {
        if ix_data.is_empty() || ix_data[0] != TXO_TAG_SET_LEN {
            return None;
        }

        // Check that the instruction targets the expected buffer account
        if accounts.is_empty() {
            return None;
        }
        let buffer_idx = accounts[0] as usize;
        if buffer_idx >= account_keys.len() || &account_keys[buffer_idx] != expected_buffer {
            return None;
        }

        // Format: [tag:1][new_len:4][resize:1][batch_id:4][height:4][finalize:1]
        if ix_data.len() < 14 {
            return None;
        }

        let new_len = u32::from_le_bytes([ix_data[1], ix_data[2], ix_data[3], ix_data[4]]);
        let batch_id = u32::from_le_bytes([ix_data[6], ix_data[7], ix_data[8], ix_data[9]]);

        Some((batch_id, new_len))
    }

    /// Parse a txo_buffer_write instruction and return batch_id, offset and data.
    fn parse_txo_buffer_write_with_batch(
        ix_data: &[u8],
        accounts: &[u8],
        account_keys: &[Pubkey],
        expected_buffer: &Pubkey,
    ) -> Option<(u32, u32, Vec<u8>)> {
        if ix_data.is_empty() || ix_data[0] != TXO_TAG_WRITE {
            return None;
        }

        // Check that the instruction targets the expected buffer account
        if accounts.is_empty() {
            return None;
        }
        let buffer_idx = accounts[0] as usize;
        if buffer_idx >= account_keys.len() || &account_keys[buffer_idx] != expected_buffer {
            return None;
        }

        if ix_data.len() < 9 {
            return None;
        }

        let batch_id = u32::from_le_bytes([ix_data[1], ix_data[2], ix_data[3], ix_data[4]]);
        let offset = u32::from_le_bytes([ix_data[5], ix_data[6], ix_data[7], ix_data[8]]);
        let data = ix_data[9..].to_vec();

        Some((batch_id, offset, data))
    }

    /// Parse a pending_mint_insert instruction to extract mint data.
    ///
    /// Instruction format:
    /// - data[0] = 3 (tag)
    /// - data[1..3] = group_idx (u16 LE)
    /// - data[3..] = mint_data (each mint is 40 bytes: 32-byte recipient + 8-byte amount)
    fn parse_pending_mint_insert(
        ix_data: &[u8],
        accounts: &[u8],
        account_keys: &[Pubkey],
        expected_buffer: &Pubkey,
    ) -> Option<(u16, Vec<PendingMint>)> {
        if ix_data.is_empty() || ix_data[0] != PM_TAG_INSERT {
            return None;
        }

        // Check that the instruction targets the expected buffer account
        if accounts.is_empty() {
            return None;
        }
        let buffer_idx = accounts[0] as usize;
        if buffer_idx >= account_keys.len() || &account_keys[buffer_idx] != expected_buffer {
            return None;
        }

        if ix_data.len() < 3 {
            return None;
        }

        let group_idx = u16::from_le_bytes([ix_data[1], ix_data[2]]);
        let mint_data = &ix_data[3..];

        // Parse mints manually to avoid alignment issues
        // Each mint is 40 bytes: 32-byte recipient + 8-byte amount (u64 LE)
        let mut mints = Vec::new();
        for chunk in mint_data.chunks_exact(PM_DA_PENDING_MINT_SIZE) {
            let mut recipient = [0u8; 32];
            recipient.copy_from_slice(&chunk[0..32]);
            let amount = u64::from_le_bytes(chunk[32..40].try_into().ok()?);
            mints.push(PendingMint { recipient, amount });
        }

        Some((group_idx, mints))
    }

    /// Assemble TXO indices from write chunks.
    fn assemble_txo_indices(writes: &HashMap<u32, Vec<u8>>, expected_size: u32) -> Vec<u32> {
        if writes.is_empty() || expected_size == 0 {
            return Vec::new();
        }

        // Assemble the raw bytes in order
        let mut raw_data = vec![0u8; expected_size as usize];
        for (offset, data) in writes {
            let start = *offset as usize;
            let end = (start + data.len()).min(raw_data.len());
            let copy_len = end - start;
            if copy_len > 0 {
                raw_data[start..end].copy_from_slice(&data[..copy_len]);
            }
        }

        // Parse as u32 indices
        let mut indices = Vec::with_capacity(raw_data.len() / 4);
        for chunk in raw_data.chunks_exact(4) {
            let idx = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            indices.push(idx);
        }

        indices
    }

    /// Parse a withdrawal request.
    fn parse_withdrawal_request(
        signature: &Signature,
        slot: u64,
        block_time: Option<i64>,
        ix_data: &[u8],
        accounts: &[u8],
        account_keys: &[Pubkey],
    ) -> Result<Option<HistoryRecord>, BridgeError> {
        use psy_doge_solana_core::instructions::doge_bridge::RequestWithdrawalInstructionData;

        let data_offset = 8;
        let data_size = std::mem::size_of::<RequestWithdrawalInstructionData>();
        if ix_data.len() < data_offset + data_size {
            return Ok(None);
        }

        let data: &RequestWithdrawalInstructionData =
            bytemuck::from_bytes(&ix_data[data_offset..data_offset + data_size]);

        let user_pubkey = if !accounts.is_empty() && (accounts[0] as usize) < account_keys.len() {
            account_keys[accounts[0] as usize]
        } else {
            Pubkey::default()
        };

        Ok(Some(HistoryRecord::WithdrawalRequest(WithdrawalRequestRecord {
            signature: *signature,
            slot,
            block_time,
            amount_sats: data.request.amount_sats,
            recipient_address: data.request.recipient_address,
            address_type: data.request.address_type,
            user_pubkey,
        })))
    }

    /// Parse a processed withdrawal.
    #[allow(unused_variables)]
    fn parse_processed_withdrawal(
        signature: &Signature,
        slot: u64,
        block_time: Option<i64>,
        ix_data: &[u8],
    ) -> Result<Option<HistoryRecord>, BridgeError> {
        use psy_doge_solana_core::instructions::doge_bridge::ProcessWithdrawalInstructionData;

        let data_offset = 8;
        let data_size = std::mem::size_of::<ProcessWithdrawalInstructionData>();
        if ix_data.len() < data_offset + data_size {
            return Ok(None);
        }

        let data: &ProcessWithdrawalInstructionData =
            bytemuck::from_bytes(&ix_data[data_offset..data_offset + data_size]);

        Ok(Some(HistoryRecord::ProcessedWithdrawal(ProcessedWithdrawalRecord {
            signature: *signature,
            slot,
            block_time,
            return_output_sighash: data.new_return_output.sighash,
            return_output_index: data.new_return_output.output_index,
            return_output_amount: data.new_return_output.amount_sats,
            spent_txo_tree_root: data.new_spent_txo_tree_root,
            next_processed_withdrawals_index: data.new_next_processed_withdrawals_index,
        })))
    }

    /// Parse a manual deposit claim.
    #[allow(unused_variables)]
    fn parse_manual_deposit(
        signature: &Signature,
        slot: u64,
        block_time: Option<i64>,
        ix_data: &[u8],
        accounts: &[u8],
        account_keys: &[Pubkey],
    ) -> Result<Option<HistoryRecord>, BridgeError> {
        use psy_doge_solana_core::instructions::doge_bridge::ProcessManualDepositInstructionData;

        let data_offset = 8;
        let data_size = std::mem::size_of::<ProcessManualDepositInstructionData>();
        if ix_data.len() < data_offset + data_size {
            return Ok(None);
        }

        let data: &ProcessManualDepositInstructionData =
            bytemuck::from_bytes(&ix_data[data_offset..data_offset + data_size]);

        Ok(Some(HistoryRecord::ManualDeposit(ManualDepositRecord {
            signature: *signature,
            slot,
            block_time,
            tx_hash: data.tx_hash,
            combined_txo_index: data.combined_txo_index,
            deposit_amount_sats: data.deposit_amount_sats,
            depositor_pubkey: data.depositor_solana_public_key,
        })))
    }
}

/// Handle for controlling a running sync.
pub struct SyncHandle {
    stop_sender: Option<tokio::sync::oneshot::Sender<()>>,
    checkpoint_receiver: mpsc::Receiver<SyncCheckpoint>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl SyncHandle {
    /// Stop the sync.
    pub fn stop(&mut self) {
        if let Some(sender) = self.stop_sender.take() {
            let _ = sender.send(());
        }
    }

    /// Get the latest checkpoint.
    pub async fn get_checkpoint(&mut self) -> Option<SyncCheckpoint> {
        self.checkpoint_receiver.recv().await
    }

    /// Wait for the sync to finish (also stops if still running).
    pub async fn join(&mut self) -> Result<(), tokio::task::JoinError> {
        self.stop();
        if let Some(handle) = self.handle.take() {
            handle.await
        } else {
            Ok(())
        }
    }

    /// Stop and forget the handle without waiting.
    pub fn abort(&mut self) {
        self.stop();
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

impl Drop for SyncHandle {
    fn drop(&mut self) {
        self.stop();
        // Note: We don't await here, the task will continue until it checks stop signal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_config_builder() {
        let program_id = Pubkey::new_unique();
        let bridge_pda = Pubkey::new_unique();
        let mint_program = Pubkey::new_unique();
        let txo_program = Pubkey::new_unique();

        let config = HistorySyncConfig::new(
            "http://localhost:8899",
            program_id,
            bridge_pda,
            mint_program,
            txo_program,
        )
        .signature_batch_size(50)
        .include_withdrawals(false);

        assert_eq!(config.signature_batch_size, 50);
        assert!(!config.include_withdrawals);
        assert!(config.include_manual_deposits);
    }

    #[test]
    fn test_checkpoint_default() {
        let checkpoint = SyncCheckpoint::default();
        assert_eq!(checkpoint.last_signature, Signature::default());
        assert_eq!(checkpoint.last_slot, 0);
        assert_eq!(checkpoint.records_processed, 0);
        assert!(checkpoint.last_block_height.is_none());
    }

    #[test]
    fn test_block_record() {
        let record = BlockRecord {
            block_height: 100,
            signature: Signature::default(),
            slot: 12345,
            block_time: Some(1234567890),
            txo_indices: vec![1, 2, 3],
            pending_mints: vec![PendingMint {
                recipient: [0u8; 32],
                amount: 1000,
            }],
            is_reorg: false,
            extra_finalized_blocks: vec![],
        };

        assert_eq!(record.block_height, 100);
        assert_eq!(record.txo_indices.len(), 3);
        assert_eq!(record.pending_mints.len(), 1);
    }

    #[test]
    fn test_history_record_variants() {
        let block = HistoryRecord::Block(BlockRecord {
            block_height: 100,
            signature: Signature::default(),
            slot: 0,
            block_time: None,
            txo_indices: vec![],
            pending_mints: vec![],
            is_reorg: false,
            extra_finalized_blocks: vec![],
        });

        match block {
            HistoryRecord::Block(b) => assert_eq!(b.block_height, 100),
            _ => panic!("Wrong variant"),
        }
    }
}
