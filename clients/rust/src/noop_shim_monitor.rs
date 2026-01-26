//! Noop Shim Monitor for tracking withdrawal VAA messages.
//!
//! This module provides a client for monitoring withdrawal messages sent through
//! the noop shim program (used for testing without real Wormhole).
//!
//! The monitor tracks:
//! - Withdrawal messages (containing Dogecoin transaction data)
//! - Pagination through historical withdrawals
//! - Real-time streaming of new withdrawal messages

use std::sync::Arc;

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::bs58;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::{UiTransactionEncoding, option_serializer::OptionSerializer};
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

use crate::config::RateLimitConfig;
use crate::errors::BridgeError;
use crate::rpc::RpcRateLimiter;

/// The noop shim program ID
pub const NOOP_SHIM_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("FwDChsHWLwbhTiYQ4Sum5mjVWswECi9cmrA11GUFUuxi");

/// The doge bridge program ID (for deriving the expected emitter PDA)
pub const DOGE_BRIDGE_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("DBjo5tqf2uwt4sg9JznSk9SBbEvsLixknN58y3trwCxJ");

/// Wormhole VAA discriminator: sha256("global:post_message")[:8]
const WORMHOLE_VAA_DISCRIMINATOR: [u8; 8] = [214, 50, 100, 209, 38, 34, 7, 76];

/// A withdrawal message captured from the noop shim.
#[derive(Debug, Clone)]
pub struct NoopShimWithdrawalMessage {
    /// Transaction signature on Solana
    pub signature: Signature,
    /// Slot where transaction was confirmed
    pub slot: u64,
    /// Block time (if available)
    pub block_time: Option<i64>,
    /// The withdrawal nonce
    pub nonce: u32,
    /// Consistency level (should be 1 = Finalized)
    pub consistency_level: u8,
    /// The sighash (first 32 bytes of payload)
    pub sighash: [u8; 32],
    /// The raw Dogecoin transaction bytes
    pub doge_tx_bytes: Vec<u8>,
    /// The emitter (bridge state PDA)
    pub emitter: Pubkey,
    /// The payer
    pub payer: Pubkey,
}

/// Configuration for the noop shim monitor.
#[derive(Debug, Clone)]
pub struct NoopShimMonitorConfig {
    /// Solana RPC URL
    pub rpc_url: String,
    /// Noop shim program ID (defaults to NOOP_SHIM_PROGRAM_ID)
    pub noop_shim_program_id: Pubkey,
    /// Expected bridge state PDA (emitter)
    pub bridge_state_pda: Pubkey,
    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,
    /// Polling interval for new transactions (milliseconds)
    pub poll_interval_ms: u64,
    /// Maximum transactions to fetch per poll
    pub batch_size: usize,
}

impl NoopShimMonitorConfig {
    /// Create a new monitor config with default settings.
    pub fn new(rpc_url: impl Into<String>, bridge_state_pda: Pubkey) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            noop_shim_program_id: NOOP_SHIM_PROGRAM_ID,
            bridge_state_pda,
            rate_limit: RateLimitConfig::default(),
            poll_interval_ms: 1000,
            batch_size: 100,
        }
    }

    /// Set a custom noop shim program ID.
    pub fn noop_shim_program_id(mut self, program_id: Pubkey) -> Self {
        self.noop_shim_program_id = program_id;
        self
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

/// Page of withdrawal messages for pagination.
#[derive(Debug, Clone)]
pub struct WithdrawalPage {
    /// The withdrawal messages in this page
    pub messages: Vec<NoopShimWithdrawalMessage>,
    /// Cursor for fetching the next page (None if no more pages)
    pub next_cursor: Option<Signature>,
    /// Whether there are more pages available
    pub has_more: bool,
}

/// Monitor for noop shim withdrawal messages.
///
/// # Example
///
/// ```ignore
/// use doge_bridge_client::noop_shim_monitor::{NoopShimMonitor, NoopShimMonitorConfig};
///
/// let config = NoopShimMonitorConfig::new(
///     "https://api.devnet.solana.com",
///     bridge_state_pda,
/// );
///
/// let monitor = NoopShimMonitor::new(config)?;
///
/// // Paginate through historical withdrawals
/// let mut cursor = None;
/// loop {
///     let page = monitor.get_withdrawals(cursor, 50).await?;
///     for msg in &page.messages {
///         println!("Withdrawal nonce={}, tx_len={}", msg.nonce, msg.doge_tx_bytes.len());
///     }
///     if !page.has_more {
///         break;
///     }
///     cursor = page.next_cursor;
/// }
///
/// // Or stream new withdrawals in real-time
/// let mut receiver = monitor.subscribe();
/// let handle = monitor.start(None).await?;
///
/// while let Some(msg) = receiver.recv().await {
///     println!("New withdrawal: nonce={}", msg.nonce);
/// }
/// ```
pub struct NoopShimMonitor {
    config: NoopShimMonitorConfig,
    rpc: Arc<RpcClient>,
    rate_limiter: Arc<RpcRateLimiter>,
    sender: mpsc::Sender<NoopShimWithdrawalMessage>,
}

impl NoopShimMonitor {
    /// Create a new noop shim monitor.
    pub fn new(config: NoopShimMonitorConfig) -> Result<Self, BridgeError> {
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

    /// Get the expected bridge state PDA (emitter).
    pub fn bridge_state_pda(&self) -> Pubkey {
        self.config.bridge_state_pda
    }

    /// Subscribe to new withdrawal messages.
    ///
    /// Returns a receiver that will receive all new withdrawal messages.
    /// The monitor must be started with `start()` for events to flow.
    ///
    /// Note: Each call creates a new channel. Only the most recent receiver
    /// will receive messages after `start()` is called.
    pub fn subscribe(&mut self) -> mpsc::Receiver<NoopShimWithdrawalMessage> {
        // Create a new channel and swap in the new sender/receiver
        // Return the NEW receiver (which is paired with the NEW sender that start() will use)
        let (sender, receiver) = mpsc::channel(1000);
        self.sender = sender;
        receiver
    }

    /// Get a page of withdrawal messages.
    ///
    /// # Arguments
    ///
    /// * `before` - Cursor from a previous page (None for the most recent)
    /// * `limit` - Maximum number of messages to return
    ///
    /// # Returns
    ///
    /// A page of withdrawal messages with pagination info.
    pub async fn get_withdrawals(
        &self,
        before: Option<Signature>,
        limit: usize,
    ) -> Result<WithdrawalPage, BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        // Query signatures for the noop shim program
        let signatures = self
            .rpc
            .get_signatures_for_address_with_config(
                &self.config.noop_shim_program_id,
                solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                    before,
                    until: None,
                    limit: Some(limit + 1), // Fetch one extra to check if there are more
                    commitment: Some(CommitmentConfig::confirmed()),
                },
            )
            .await?;

        let has_more = signatures.len() > limit;
        let signatures_to_process: Vec<_> = signatures.into_iter().take(limit).collect();

        let mut messages = Vec::new();
        let mut last_sig = None;

        for sig_info in &signatures_to_process {
            if let Ok(sig) = sig_info.signature.parse::<Signature>() {
                last_sig = Some(sig);
                if let Some(msg) = self.parse_transaction(&sig).await? {
                    // Only include messages from the expected emitter
                    if msg.emitter == self.config.bridge_state_pda {
                        messages.push(msg);
                    }
                }
            }
        }

        Ok(WithdrawalPage {
            messages,
            next_cursor: if has_more { last_sig } else { None },
            has_more,
        })
    }

    /// Fetch all withdrawal messages since a given signature.
    ///
    /// Returns messages in chronological order (oldest first).
    pub async fn fetch_since(
        &self,
        after_signature: Option<Signature>,
        limit: Option<usize>,
    ) -> Result<Vec<NoopShimWithdrawalMessage>, BridgeError> {
        let mut messages = Vec::new();
        // `before` is used for pagination - start from most recent, paginate backwards
        let mut before_signature: Option<Signature> = None;
        let limit = limit.unwrap_or(usize::MAX);

        while messages.len() < limit {
            let _guard = self.rate_limiter.acquire().await?;

            let signatures = self
                .rpc
                .get_signatures_for_address_with_config(
                    &self.config.noop_shim_program_id,
                    solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                        // `before` - pagination cursor, start from this sig going backwards
                        before: before_signature,
                        // `until` - stop when reaching this sig (our checkpoint)
                        until: after_signature,
                        limit: Some(self.config.batch_size.min(limit - messages.len())),
                        commitment: Some(CommitmentConfig::confirmed()),
                    },
                )
                .await?;

            if signatures.is_empty() {
                break;
            }

            for sig_info in &signatures {
                tracing::info!("fetching tx for signature: {}", sig_info.signature);
                if let Ok(sig) = sig_info.signature.parse::<Signature>() {
                    if let Some(msg) = self.parse_transaction(&sig).await? {
                        if msg.emitter == self.config.bridge_state_pda {
                            messages.push(msg);
                        }
                    }
                }
            }

            // Update pagination cursor to oldest sig in this batch to fetch older txs next
            before_signature = signatures.last().and_then(|s| s.signature.parse().ok());
        }

        // Reverse to get chronological order (oldest first)
        messages.reverse();
        Ok(messages)
    }

    /// Start monitoring for new withdrawal messages.
    ///
    /// If `after_signature` is None, starts from the most recent transaction.
    pub async fn start(
        &self,
        after_signature: Option<Signature>,
    ) -> Result<NoopShimMonitorHandle, BridgeError> {
        let (stop_sender, stop_receiver) = tokio::sync::oneshot::channel();

        let rpc = self.rpc.clone();
        let rate_limiter = self.rate_limiter.clone();
        let sender = self.sender.clone();
        let config = self.config.clone();

        let handle = tokio::spawn(async move {
            Self::monitor_loop(rpc, rate_limiter, sender, config, after_signature, stop_receiver)
                .await
        });

        Ok(NoopShimMonitorHandle {
            stop_sender: Some(stop_sender),
            handle: Some(handle),
        })
    }

    /// Internal monitor loop.
    async fn monitor_loop(
        rpc: Arc<RpcClient>,
        rate_limiter: Arc<RpcRateLimiter>,
        sender: mpsc::Sender<NoopShimWithdrawalMessage>,
        config: NoopShimMonitorConfig,
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
                            tracing::error!("NoopShim monitor poll error: {}", e);
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
        sender: &mpsc::Sender<NoopShimWithdrawalMessage>,
        config: &NoopShimMonitorConfig,
        after_signature: &mut Option<Signature>,
    ) -> Result<(), BridgeError> {
        let _guard = rate_limiter.acquire().await?;

        let signatures = rpc
            .get_signatures_for_address_with_config(
                &config.noop_shim_program_id,
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
                if let Some(msg) =
                    Self::parse_transaction_static(rpc, rate_limiter, config, &sig).await?
                {
                    if msg.emitter == config.bridge_state_pda {
                        if sender.send(msg).await.is_err() {
                            // Receiver dropped, stop
                            return Ok(());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Parse a transaction to extract withdrawal message.
    async fn parse_transaction(
        &self,
        signature: &Signature,
    ) -> Result<Option<NoopShimWithdrawalMessage>, BridgeError> {
        Self::parse_transaction_static(&self.rpc, &self.rate_limiter, &self.config, signature).await
    }

    /// Parse a transaction to extract withdrawal message (static version).
    async fn parse_transaction_static(
        rpc: &RpcClient,
        rate_limiter: &RpcRateLimiter,
        config: &NoopShimMonitorConfig,
        signature: &Signature,
    ) -> Result<Option<NoopShimWithdrawalMessage>, BridgeError> {
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
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(&data)
                    .map_err(|e| BridgeError::InvalidInput(format!("Failed to decode tx: {}", e)))?;
                bincode::deserialize::<solana_sdk::transaction::VersionedTransaction>(&bytes)
                    .map_err(|e| {
                        BridgeError::InvalidInput(format!("Failed to deserialize tx: {}", e))
                    })?
            }
            _ => return Ok(None),
        };

        // Verify transaction signatures to ensure the RPC isn't lying about signers
        // Serialize the message to get the bytes that were signed
        let message_bytes = transaction.message.serialize();
        let static_keys = transaction.message.static_account_keys();
        let num_required_signatures = transaction.message.header().num_required_signatures as usize;

        // Verify each required signature
        for i in 0..num_required_signatures {
            if i >= transaction.signatures.len() || i >= static_keys.len() {
                tracing::warn!(
                    "Transaction {} missing signature {} of {}",
                    signature,
                    i,
                    num_required_signatures
                );
                return Ok(None);
            }

            let sig = &transaction.signatures[i];
            let pubkey = &static_keys[i];

            // Verify the ed25519 signature
            if !sig.verify(pubkey.as_ref(), &message_bytes) {
                tracing::error!(
                    "Transaction {} has invalid signature from {} (signer {})",
                    signature,
                    pubkey,
                    i
                );
                return Err(BridgeError::InvalidInput(format!(
                    "Invalid signature from {} on transaction {}",
                    pubkey, signature
                )));
            }
        }

        tracing::debug!(
            "Verified {} signatures on transaction {}",
            num_required_signatures,
            signature
        );

        // Find instructions to the noop shim program
        // Note: The noop shim is called via CPI, so we need to look at inner instructions
        let message = transaction.message;
        let account_keys = message.static_account_keys();

        // Get inner instructions from transaction metadata
        let inner_instructions = match &tx.transaction.meta {
            Some(meta) => match &meta.inner_instructions {
                OptionSerializer::Some(inner) => inner.clone(),
                _ => vec![],
            },
            None => vec![],
        };

        // Look through inner instructions for noop shim CPI calls
        for ui_inner in &inner_instructions {
            for ui_ix in &ui_inner.instructions {
                // Extract the compiled instruction from the UiInstruction enum
                let (program_id_index, accounts, data): (u8, Vec<u8>, Vec<u8>) = match ui_ix {
                    solana_transaction_status::UiInstruction::Compiled(compiled) => {
                        let data = match bs58::decode(&compiled.data).into_vec() {
                            Ok(d) => d,
                            Err(_) => continue,
                        };
                        (compiled.program_id_index, compiled.accounts.clone(), data)
                    }
                    _ => continue,
                };

                let program_idx = program_id_index as usize;
                if program_idx >= account_keys.len() {
                    continue;
                }

                if account_keys[program_idx] != config.noop_shim_program_id {
                    continue;
                }

                let ix_data = &data;

                // Parse the wormhole-style instruction data
                // Format: [8 bytes discriminator][4 bytes nonce][1 byte consistency][4 bytes payload_len][payload]
                if ix_data.len() < 17 {
                    continue;
                }

                // Check discriminator
                if ix_data[0..8] != WORMHOLE_VAA_DISCRIMINATOR {
                    continue;
                }

                let nonce = u32::from_le_bytes(ix_data[8..12].try_into().unwrap());
                let consistency_level = ix_data[12];
                let payload_len = u32::from_le_bytes(ix_data[13..17].try_into().unwrap()) as usize;

                if ix_data.len() < 17 + payload_len {
                    continue;
                }

                let payload = &ix_data[17..17 + payload_len];

                // Payload format: [32 bytes sighash][remaining: doge tx bytes]
                if payload.len() < 32 {
                    continue;
                }

                let mut sighash = [0u8; 32];
                sighash.copy_from_slice(&payload[0..32]);
                let doge_tx_bytes = payload[32..].to_vec();

                // Get emitter (account index 2) and payer (account index 4)
                let emitter = if accounts.len() > 2 {
                    let idx = accounts[2] as usize;
                    if idx < account_keys.len() {
                        account_keys[idx]
                    } else {
                        continue;
                    }
                } else {
                    continue;
                };

                let payer = if accounts.len() > 4 {
                    let idx = accounts[4] as usize;
                    if idx < account_keys.len() {
                        account_keys[idx]
                    } else {
                        Pubkey::default()
                    }
                } else {
                    Pubkey::default()
                };

                return Ok(Some(NoopShimWithdrawalMessage {
                    signature: *signature,
                    slot,
                    block_time,
                    nonce,
                    consistency_level,
                    sighash,
                    doge_tx_bytes,
                    emitter,
                    payer,
                }));
            }
        }

        Ok(None)
    }
}

/// Handle for controlling a running noop shim monitor.
pub struct NoopShimMonitorHandle {
    stop_sender: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl NoopShimMonitorHandle {
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

impl Drop for NoopShimMonitorHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let bridge_pda = Pubkey::new_unique();

        let config = NoopShimMonitorConfig::new("http://localhost:8899", bridge_pda)
            .poll_interval_ms(500)
            .batch_size(50);

        assert_eq!(config.poll_interval_ms, 500);
        assert_eq!(config.batch_size, 50);
        assert_eq!(config.bridge_state_pda, bridge_pda);
        assert_eq!(config.noop_shim_program_id, NOOP_SHIM_PROGRAM_ID);
    }

    #[test]
    fn test_withdrawal_message_struct() {
        let msg = NoopShimWithdrawalMessage {
            signature: Signature::default(),
            slot: 100,
            block_time: Some(12345),
            nonce: 42,
            consistency_level: 1,
            sighash: [0u8; 32],
            doge_tx_bytes: vec![1, 2, 3, 4],
            emitter: Pubkey::new_unique(),
            payer: Pubkey::new_unique(),
        };

        assert_eq!(msg.nonce, 42);
        assert_eq!(msg.consistency_level, 1);
        assert_eq!(msg.doge_tx_bytes.len(), 4);
    }

    #[test]
    fn test_program_ids() {
        // Verify the program IDs are valid base58
        assert_eq!(
            NOOP_SHIM_PROGRAM_ID.to_string(),
            "FwDChsHWLwbhTiYQ4Sum5mjVWswECi9cmrA11GUFUuxi"
        );
        assert_eq!(
            DOGE_BRIDGE_PROGRAM_ID.to_string(),
            "DBjo5tqf2uwt4sg9JznSk9SBbEvsLixknN58y3trwCxJ"
        );
    }
}
