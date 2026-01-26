//! Bridge monitoring client for streaming events.
//!
//! This module provides a client for monitoring bridge events in real-time:
//! - Withdrawal requests
//! - Manually claimed deposits
//! - Processed withdrawals
//!
//! The monitor is designed for bridge node operators who need to track
//! all bridge activity efficiently without hitting rate limits.

use std::sync::Arc;

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::UiTransactionEncoding;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use crate::config::RateLimitConfig;
use crate::errors::BridgeError;
use crate::rpc::RpcRateLimiter;

use psy_doge_solana_core::instructions::doge_bridge::{
    DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT, DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL,
    DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL, RequestWithdrawalInstructionData,
    ProcessWithdrawalInstructionData, ProcessManualDepositInstructionData,
};

/// Event types emitted by the bridge monitor.
#[derive(Debug, Clone)]
pub enum BridgeEvent {
    /// A user requested a withdrawal (burned tokens).
    WithdrawalRequested(WithdrawalRequestedEvent),
    /// A withdrawal was processed (sent to Dogecoin).
    WithdrawalProcessed(WithdrawalProcessedEvent),
    /// A user manually claimed a deposit.
    ManualDepositClaimed(ManualDepositClaimedEvent),
    /// A block transition occurred (new finalized block).
    BlockTransition(BlockTransitionEvent),
}

/// Event when a user requests a withdrawal.
#[derive(Debug, Clone)]
pub struct WithdrawalRequestedEvent {
    /// Transaction signature
    pub signature: Signature,
    /// Slot where transaction was confirmed
    pub slot: u64,
    /// Block time (if available)
    pub block_time: Option<i64>,
    /// Withdrawal amount in satoshis
    pub amount_sats: u64,
    /// Recipient Dogecoin address (20 bytes)
    pub recipient_address: [u8; 20],
    /// Address type (0 = P2PKH, 1 = P2SH)
    pub address_type: u32,
    /// User's Solana pubkey who requested
    pub user_pubkey: Pubkey,
    /// Index in the withdrawal queue
    pub withdrawal_index: u64,
}

/// Event when a withdrawal is processed (sent to Dogecoin).
#[derive(Debug, Clone)]
pub struct WithdrawalProcessedEvent {
    /// Transaction signature
    pub signature: Signature,
    /// Slot where transaction was confirmed
    pub slot: u64,
    /// Block time (if available)
    pub block_time: Option<i64>,
    /// New return output sighash
    pub new_return_output_sighash: [u8; 32],
    /// New return output index
    pub new_return_output_index: u64,
    /// New return output amount
    pub new_return_output_amount: u64,
    /// New spent TXO tree root
    pub new_spent_txo_tree_root: [u8; 32],
    /// New next processed withdrawals index
    pub new_next_processed_withdrawals_index: u64,
}

/// Event when a user manually claims a deposit.
#[derive(Debug, Clone)]
pub struct ManualDepositClaimedEvent {
    /// Transaction signature
    pub signature: Signature,
    /// Slot where transaction was confirmed
    pub slot: u64,
    /// Block time (if available)
    pub block_time: Option<i64>,
    /// Dogecoin transaction hash
    pub tx_hash: [u8; 32],
    /// Combined TXO index (encodes block, tx, output)
    pub combined_txo_index: u64,
    /// Deposit amount in satoshis
    pub deposit_amount_sats: u64,
    /// Depositor's Solana public key
    pub depositor_pubkey: [u8; 32],
    /// User who claimed (signer)
    pub claimer_pubkey: Pubkey,
}

/// Event when a block transition occurs.
#[derive(Debug, Clone)]
pub struct BlockTransitionEvent {
    /// Transaction signature
    pub signature: Signature,
    /// Slot where transaction was confirmed
    pub slot: u64,
    /// Block time (if available)
    pub block_time: Option<i64>,
    /// New finalized block height
    pub block_height: u32,
    /// Whether this was a reorg
    pub is_reorg: bool,
}

/// Configuration for the bridge monitor.
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// Solana RPC URL
    pub rpc_url: String,
    /// Bridge program ID
    pub program_id: Pubkey,
    /// Bridge state PDA
    pub bridge_state_pda: Pubkey,
    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,
    /// Polling interval for new transactions (milliseconds)
    pub poll_interval_ms: u64,
    /// Maximum transactions to fetch per poll
    pub batch_size: usize,
}

impl MonitorConfig {
    /// Create a new monitor config with default settings.
    pub fn new(rpc_url: impl Into<String>, program_id: Pubkey, bridge_state_pda: Pubkey) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            program_id,
            bridge_state_pda,
            rate_limit: RateLimitConfig::default(),
            poll_interval_ms: 1000,
            batch_size: 100,
        }
    }

    /// Set the polling interval.
    pub fn poll_interval_ms(mut self, ms: u64) -> Self {
        self.poll_interval_ms = ms;
        self
    }

    /// Set the batch size.
    pub fn batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Set the rate limit configuration.
    pub fn rate_limit(mut self, config: RateLimitConfig) -> Self {
        self.rate_limit = config;
        self
    }
}

/// Bridge monitor for streaming events.
///
/// Monitors the bridge program for:
/// - Withdrawal requests
/// - Processed withdrawals
/// - Manual deposit claims
/// - Block transitions
///
/// # Example
///
/// ```ignore
/// use doge_bridge_client::monitor::{BridgeMonitor, MonitorConfig, BridgeEvent};
///
/// let config = MonitorConfig::new(
///     "https://api.mainnet-beta.solana.com",
///     program_id,
///     bridge_state_pda,
/// );
///
/// let monitor = BridgeMonitor::new(config)?;
/// let mut receiver = monitor.subscribe();
///
/// while let Some(event) = receiver.recv().await {
///     match event {
///         BridgeEvent::WithdrawalRequested(e) => {
///             println!("Withdrawal requested: {} sats", e.amount_sats);
///         }
///         BridgeEvent::WithdrawalProcessed(e) => {
///             println!("Withdrawal processed: sig={}", e.signature);
///         }
///         _ => {}
///     }
/// }
/// ```
pub struct BridgeMonitor {
    config: MonitorConfig,
    rpc: Arc<RpcClient>,
    rate_limiter: Arc<RpcRateLimiter>,
    sender: mpsc::Sender<BridgeEvent>,
}

impl BridgeMonitor {
    /// Create a new bridge monitor.
    pub fn new(config: MonitorConfig) -> Result<Self, BridgeError> {
        let rpc = Arc::new(RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        ));
        let rate_limiter = Arc::new(RpcRateLimiter::new(config.rate_limit.clone()));
        let (sender, _receiver) = mpsc::channel(1000);

        Ok(Self {
            config,
            rpc,
            rate_limiter,
            sender,
        })
    }

    /// Subscribe to bridge events.
    ///
    /// Returns a receiver that will receive all bridge events.
    /// The monitor must be started with `start()` for events to flow.
    ///
    /// Note: Each call creates a new channel. Only the most recent receiver
    /// will receive messages after `start()` is called.
    pub fn subscribe(&mut self) -> mpsc::Receiver<BridgeEvent> {
        // Create a new channel and return the receiver
        // The sender is stored so start() will use it
        let (sender, receiver) = mpsc::channel(1000);
        self.sender = sender;
        receiver
    }

    /// Start monitoring from the given signature.
    ///
    /// If `after_signature` is None, starts from the most recent transaction.
    /// Returns a handle that can be used to stop the monitor.
    pub async fn start(
        &self,
        after_signature: Option<Signature>,
    ) -> Result<MonitorHandle, BridgeError> {
        let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel();

        let rpc = self.rpc.clone();
        let rate_limiter = self.rate_limiter.clone();
        let sender = self.sender.clone();
        let config = self.config.clone();

        let handle = tokio::spawn(async move {
            Self::monitor_loop(rpc, rate_limiter, sender, config, after_signature, stop_receiver)
                .await
        });

        Ok(MonitorHandle {
            stop_sender: Some(stop_sender),
            handle: Some(handle),
        })
    }

    /// Fetch all transactions since the given signature.
    ///
    /// This is useful for catching up on missed events.
    pub async fn fetch_since(
        &self,
        after_signature: Option<Signature>,
        limit: Option<usize>,
    ) -> Result<Vec<BridgeEvent>, BridgeError> {
        let mut events = Vec::new();
        // `before` is used for pagination - start from most recent, paginate backwards
        let mut before_signature: Option<Signature> = None;
        let limit = limit.unwrap_or(usize::MAX);

        while events.len() < limit {
            let _guard = self.rate_limiter.acquire().await?;

            let signatures = self
                .rpc
                .get_signatures_for_address_with_config(
                    &self.config.bridge_state_pda,
                    solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                        // `before` - pagination cursor, start from this sig going backwards
                        before: before_signature,
                        // `until` - stop when reaching this sig (our checkpoint)
                        until: after_signature,
                        limit: Some(self.config.batch_size.min(limit - events.len())),
                        commitment: Some(CommitmentConfig::confirmed()),
                    },
                )
                .await?;

            if signatures.is_empty() {
                break;
            }

            for sig_info in &signatures {
                tracing::info!("fetching tx for signature: {}", sig_info.signature);
                if let Some(event) = self.parse_transaction(&sig_info.signature).await? {
                    events.push(event);
                }
            }

            // Update pagination cursor to oldest sig in this batch to fetch older txs next
            before_signature = signatures.last().and_then(|s| {
                s.signature.parse().ok()
            });
        }

        // Reverse to get chronological order (oldest first)
        events.reverse();
        Ok(events)
    }

    /// Internal monitor loop.
    async fn monitor_loop(
        rpc: Arc<RpcClient>,
        rate_limiter: Arc<RpcRateLimiter>,
        sender: mpsc::Sender<BridgeEvent>,
        config: MonitorConfig,
        mut after_signature: Option<Signature>,
        mut stop_receiver: tokio::sync::oneshot::Receiver<()>,
    ) {
        let mut poll_interval = interval(Duration::from_millis(config.poll_interval_ms));

        loop {
            tokio::select! {
                _ = &mut stop_receiver => {
                    break;
                }
                _ = poll_interval.tick() => {
                    match Self::poll_once(&rpc, &rate_limiter, &sender, &config, &mut after_signature).await {
                        Ok(_) => {}
                        Err(e) => {
                            tracing::error!("Monitor poll error: {}", e);
                        }
                    }
                }
            }
        }
    }

    /// Poll for new transactions once.
    async fn poll_once(
        rpc: &RpcClient,
        rate_limiter: &RpcRateLimiter,
        sender: &mpsc::Sender<BridgeEvent>,
        config: &MonitorConfig,
        after_signature: &mut Option<Signature>,
    ) -> Result<(), BridgeError> {
        let _guard = rate_limiter.acquire().await?;

        let signatures = rpc
            .get_signatures_for_address_with_config(
                &config.bridge_state_pda,
                solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                    before: None,
                    until: *after_signature,
                    limit: Some(config.batch_size),
                    commitment: Some(CommitmentConfig::confirmed()),
                },
            )
            .await?;

        if signatures.is_empty() {
            return Ok(());
        }

        // Update checkpoint to newest signature BEFORE processing
        // (first in list since API returns newest-first)
        if let Some(newest) = signatures.first() {
            if let Ok(sig) = newest.signature.parse::<Signature>() {
                *after_signature = Some(sig);
            }
        }

        // Process in reverse (oldest first) for chronological order
        for sig_info in signatures.iter().rev() {
            if let Ok(sig) = sig_info.signature.parse::<Signature>() {
                if let Some(event) = Self::parse_transaction_static(rpc, rate_limiter, &config.program_id, &sig).await? {
                    if sender.send(event).await.is_err() {
                        // Receiver dropped, stop
                        return Ok(());
                    }
                }
            }
        }

        Ok(())
    }

    /// Parse a transaction to extract bridge events.
    async fn parse_transaction(&self, signature_str: &str) -> Result<Option<BridgeEvent>, BridgeError> {
        let sig = signature_str.parse::<Signature>().map_err(|e| {
            BridgeError::InvalidInput(format!("Invalid signature: {}", e))
        })?;

        Self::parse_transaction_static(&self.rpc, &self.rate_limiter, &self.config.program_id, &sig).await
    }

    /// Parse a transaction to extract bridge events (static version).
    async fn parse_transaction_static(
        rpc: &RpcClient,
        rate_limiter: &RpcRateLimiter,
        program_id: &Pubkey,
        signature: &Signature,
    ) -> Result<Option<BridgeEvent>, BridgeError> {
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

            if account_keys[program_idx] != *program_id {
                continue;
            }

            let ix_data = &ix.data;
            if ix_data.is_empty() {
                continue;
            }

            // Parse based on instruction discriminator
            // The discriminator is at offset 4 (after 4-byte alignment prefix)
            if ix_data.len() < 5 {
                continue;
            }
            let discriminator = ix_data[4];

            match discriminator {
                DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL => {
                    if let Some(event) = Self::parse_withdrawal_request(
                        signature,
                        slot,
                        block_time,
                        ix_data,
                        &ix.accounts,
                        account_keys,
                    ) {
                        return Ok(Some(BridgeEvent::WithdrawalRequested(event)));
                    }
                }
                DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL => {
                    if let Some(event) = Self::parse_process_withdrawal(
                        signature,
                        slot,
                        block_time,
                        ix_data,
                    ) {
                        return Ok(Some(BridgeEvent::WithdrawalProcessed(event)));
                    }
                }
                DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT => {
                    if let Some(event) = Self::parse_manual_deposit(
                        signature,
                        slot,
                        block_time,
                        ix_data,
                        &ix.accounts,
                        account_keys,
                    ) {
                        return Ok(Some(BridgeEvent::ManualDepositClaimed(event)));
                    }
                }
                _ => {}
            }
        }

        Ok(None)
    }

    /// Parse a withdrawal request instruction.
    fn parse_withdrawal_request(
        signature: &Signature,
        slot: u64,
        block_time: Option<i64>,
        ix_data: &[u8],
        accounts: &[u8],
        account_keys: &[Pubkey],
    ) -> Option<WithdrawalRequestedEvent> {
        // Skip 8-byte alignment header
        if ix_data.len() < 8 + std::mem::size_of::<RequestWithdrawalInstructionData>() {
            return None;
        }

        let data: &RequestWithdrawalInstructionData =
            bytemuck::from_bytes(&ix_data[8..8 + std::mem::size_of::<RequestWithdrawalInstructionData>()]);

        // The user signer is typically the first account
        let user_pubkey = if !accounts.is_empty() && (accounts[0] as usize) < account_keys.len() {
            account_keys[accounts[0] as usize]
        } else {
            Pubkey::default()
        };

        Some(WithdrawalRequestedEvent {
            signature: *signature,
            slot,
            block_time,
            amount_sats: data.request.amount_sats,
            recipient_address: data.request.recipient_address,
            address_type: data.request.address_type,
            user_pubkey,
            withdrawal_index: 0, // Would need to read from state
        })
    }

    /// Parse a process withdrawal instruction.
    fn parse_process_withdrawal(
        signature: &Signature,
        slot: u64,
        block_time: Option<i64>,
        ix_data: &[u8],
    ) -> Option<WithdrawalProcessedEvent> {
        // Skip 8-byte alignment header
        if ix_data.len() < 8 + std::mem::size_of::<ProcessWithdrawalInstructionData>() {
            return None;
        }

        let data: &ProcessWithdrawalInstructionData =
            bytemuck::from_bytes(&ix_data[8..8 + std::mem::size_of::<ProcessWithdrawalInstructionData>()]);

        Some(WithdrawalProcessedEvent {
            signature: *signature,
            slot,
            block_time,
            new_return_output_sighash: data.new_return_output.sighash,
            new_return_output_index: data.new_return_output.output_index,
            new_return_output_amount: data.new_return_output.amount_sats,
            new_spent_txo_tree_root: data.new_spent_txo_tree_root,
            new_next_processed_withdrawals_index: data.new_next_processed_withdrawals_index,
        })
    }

    /// Parse a manual deposit claim instruction.
    fn parse_manual_deposit(
        signature: &Signature,
        slot: u64,
        block_time: Option<i64>,
        ix_data: &[u8],
        accounts: &[u8],
        account_keys: &[Pubkey],
    ) -> Option<ManualDepositClaimedEvent> {
        // Skip 8-byte alignment header
        if ix_data.len() < 8 + std::mem::size_of::<ProcessManualDepositInstructionData>() {
            return None;
        }

        let data: &ProcessManualDepositInstructionData =
            bytemuck::from_bytes(&ix_data[8..8 + std::mem::size_of::<ProcessManualDepositInstructionData>()]);

        // The user signer is typically the first account
        let claimer_pubkey = if !accounts.is_empty() && (accounts[0] as usize) < account_keys.len() {
            account_keys[accounts[0] as usize]
        } else {
            Pubkey::default()
        };

        Some(ManualDepositClaimedEvent {
            signature: *signature,
            slot,
            block_time,
            tx_hash: data.tx_hash,
            combined_txo_index: data.combined_txo_index,
            deposit_amount_sats: data.deposit_amount_sats,
            depositor_pubkey: data.depositor_solana_public_key,
            claimer_pubkey,
        })
    }
}

/// Handle for controlling a running monitor.
pub struct MonitorHandle {
    stop_sender: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl MonitorHandle {
    /// Stop the monitor.
    pub fn stop(&mut self) {
        if let Some(sender) = self.stop_sender.take() {
            let _ = sender.send(());
        }
    }

    /// Wait for the monitor to finish (also stops if still running).
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

impl Drop for MonitorHandle {
    fn drop(&mut self) {
        self.stop();
        // Note: We don't await here, the task will continue until it checks stop signal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_config_builder() {
        let program_id = Pubkey::new_unique();
        let bridge_pda = Pubkey::new_unique();

        let config = MonitorConfig::new("http://localhost:8899", program_id, bridge_pda)
            .poll_interval_ms(500)
            .batch_size(50);

        assert_eq!(config.poll_interval_ms, 500);
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.program_id, program_id);
    }

    #[test]
    fn test_event_types() {
        // Ensure event types are properly defined
        let event = BridgeEvent::WithdrawalRequested(WithdrawalRequestedEvent {
            signature: Signature::default(),
            slot: 100,
            block_time: Some(12345),
            amount_sats: 1_000_000,
            recipient_address: [0u8; 20],
            address_type: 0,
            user_pubkey: Pubkey::new_unique(),
            withdrawal_index: 5,
        });

        match event {
            BridgeEvent::WithdrawalRequested(e) => {
                assert_eq!(e.amount_sats, 1_000_000);
                assert_eq!(e.slot, 100);
            }
            _ => panic!("Wrong event type"),
        }
    }
}
