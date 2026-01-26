//! Main BridgeClient implementation.
//!
//! Provides a clean, abstracted interface to all bridge operations
//! with built-in rate limiting, retries, and parallel buffer building.

use std::sync::Arc;

use async_trait::async_trait;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};

use crate::{
    BridgeEvent, BridgeMonitor, MonitorConfig, api::{BridgeApi, ManualClaimApi, OperatorApi, WithdrawalApi}, buffer::ParallelBufferManager, config::{BridgeClientConfig, BridgeClientConfigBuilder}, errors::BridgeError, rpc::{RetryExecutor, RpcRateLimiter}, types::{
        CompactBridgeZKProof, DepositTxOutputRecord, FinalizedBlockMintTxoInfo,
        InitializeBridgeParams, PendingMint, ProcessMintsResult, PsyBridgeHeader,
        PsyBridgeProgramState, PsyReturnTxOutput, PsyWithdrawalChainSnapshot,
    }
};

/// Main client for interacting with the Doge bridge on Solana.
///
/// Provides a clean API for all bridge operations with built-in:
/// - Rate limiting for RPC requests
/// - Automatic retry with exponential backoff
/// - Parallel buffer building
///
/// # Example
///
/// ```ignore
/// use doge_bridge_client::{BridgeClient, BridgeApi};
///
/// let client = BridgeClient::new(
///     "https://api.mainnet-beta.solana.com",
///     &operator_private_key,
///     &payer_private_key,
///     bridge_state_pda,
/// )?;
///
/// let state = client.get_current_bridge_state().await?;
/// ```
pub struct BridgeClient {
    pub(crate) config: BridgeClientConfig,
    pub(crate) rpc: Arc<RpcClient>,
    pub(crate) rate_limiter: Arc<RpcRateLimiter>,
    pub(crate) retry_executor: RetryExecutor,
    pub(crate) buffer_manager: ParallelBufferManager,
    /// Cached DOGE mint address
    doge_mint_cache: tokio::sync::RwLock<Option<Pubkey>>,
}

impl BridgeClient {
    /// Create a new bridge client with minimal configuration.
    ///
    /// # Arguments
    ///
    /// * `rpc_url` - Solana RPC URL
    /// * `operator_private_key` - 64-byte operator keypair
    /// * `payer_private_key` - 64-byte payer keypair
    /// * `bridge_state_pda` - Bridge state PDA address
    /// * `wormhole_core_program_id` - Wormhole core program ID
    /// * `wormhole_shim_program_id` - Wormhole shim program ID
    pub fn new(
        rpc_url: &str,
        operator_private_key: &[u8],
        payer_private_key: &[u8],
        bridge_state_pda: Pubkey,
        wormhole_core_program_id: Pubkey,
        wormhole_shim_program_id: Pubkey,
    ) -> Result<Self, BridgeError> {
        let operator = Keypair::from_bytes(operator_private_key).map_err(|e| {
            BridgeError::InvalidInput(format!("Invalid operator keypair: {}", e))
        })?;

        let payer = Keypair::from_bytes(payer_private_key).map_err(|e| {
            BridgeError::InvalidInput(format!("Invalid payer keypair: {}", e))
        })?;

        let config = BridgeClientConfigBuilder::new()
            .rpc_url(rpc_url)
            .bridge_state_pda(bridge_state_pda)
            .operator(operator)
            .payer(payer)
            .wormhole_core_program_id(wormhole_core_program_id)
            .wormhole_shim_program_id(wormhole_shim_program_id)
            .build()
            .map_err(|e| BridgeError::InvalidConfig {
                message: e.to_string(),
            })?;

        Self::with_config(config)
    }

    /// Create a new bridge client with full configuration.
    pub fn with_config(config: BridgeClientConfig) -> Result<Self, BridgeError> {
        let rpc = Arc::new(RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        ));

        let rate_limiter = Arc::new(RpcRateLimiter::new(config.rate_limit.clone()));
        let retry_executor = RetryExecutor::new(config.retry.clone());

        let buffer_manager = ParallelBufferManager::new(
            rpc.clone(),
            config.payer.clone(),
            config.operator.clone(),
            rate_limiter.clone(),
            retry_executor.clone(),
            config.parallelism.clone(),
        );

        Ok(Self {
            config,
            rpc,
            rate_limiter,
            retry_executor,
            buffer_manager,
            doge_mint_cache: tokio::sync::RwLock::new(None),
        })
    }

    /// Get the DOGE mint address (cached after first fetch).
    pub async fn get_doge_mint(&self) -> Result<Pubkey, BridgeError> {
        // Check cache first
        {
            let cache = self.doge_mint_cache.read().await;
            if let Some(mint) = *cache {
                return Ok(mint);
            }
        }

        // Check config
        if let Some(mint) = self.config.doge_mint {
            let mut cache = self.doge_mint_cache.write().await;
            *cache = Some(mint);
            return Ok(mint);
        }

        // Fetch from chain
        let mint = self.get_doge_mint_from_state().await?;
        let mut cache = self.doge_mint_cache.write().await;
        *cache = Some(mint);
        Ok(mint)
    }

    /// Get the bridge state PDA.
    pub fn bridge_state_pda(&self) -> Pubkey {
        self.config.bridge_state_pda
    }

    /// Get the operator public key.
    pub fn operator_pubkey(&self) -> Pubkey {
        self.config.operator.pubkey()
    }

    /// Get the payer public key.
    pub fn payer_pubkey(&self) -> Pubkey {
        self.config.payer.pubkey()
    }

    /// Send a transaction and wait for confirmation.
    pub(crate) async fn send_and_confirm(
        &self,
        instructions: &[Instruction],
        extra_signers: &[&Keypair],
    ) -> Result<Signature, BridgeError> {
        let _guard = self.rate_limiter.acquire().await?;

        self.retry_executor
            .execute(|| self.send_tx(instructions, extra_signers))
            .await
    }

    /// Send a transaction (internal).
    async fn send_tx(
        &self,
        instructions: &[Instruction],
        extra_signers: &[&Keypair],
    ) -> Result<Signature, BridgeError> {
        let recent_blockhash = self.rpc.get_latest_blockhash().await?;

        let mut signers: Vec<&Keypair> = vec![self.config.payer.as_ref()];
        signers.extend(extra_signers);

        let tx = Transaction::new_signed_with_payer(
            instructions,
            Some(&self.config.payer.pubkey()),
            &signers,
            recent_blockhash,
        );

        let signature = self.rpc.send_and_confirm_transaction(&tx).await?;
        Ok(signature)
    }
}

// Implement the BridgeApi trait
#[async_trait]
impl BridgeApi for BridgeClient {
    async fn get_current_bridge_state(&self) -> Result<PsyBridgeProgramState, BridgeError> {
        self.get_current_bridge_state_impl().await
    }

    async fn get_manual_deposits_at(
        &self,
        next_processed_manual_deposit_index: u64,
        max_count: u32,
    ) -> Result<Vec<DepositTxOutputRecord>, BridgeError> {
        self.get_manual_deposits_at_impl(next_processed_manual_deposit_index, max_count)
            .await
    }

    async fn process_remaining_pending_mints_groups(
        &self,
        pending_mints: &[PendingMint],
        mint_buffer_account: Pubkey,
        mint_buffer_bump: u8,
    ) -> Result<ProcessMintsResult, BridgeError> {
        self.process_remaining_pending_mints_groups_impl(
            pending_mints,
            mint_buffer_account,
            mint_buffer_bump,
        )
        .await
    }

    async fn process_remaining_pending_mints_groups_auto_advance(
        &self,
        pending_mints: &[PendingMint],
        mint_buffer_account: Pubkey,
        mint_buffer_bump: u8,
        txo_buffer_account: Pubkey,
        txo_buffer_bump: u8,
    ) -> Result<ProcessMintsResult, BridgeError> {
        self.process_remaining_pending_mints_groups_auto_advance_impl(
            pending_mints,
            mint_buffer_account,
            mint_buffer_bump,
            txo_buffer_account,
            txo_buffer_bump,
        )
        .await
    }

    async fn process_block_transition(
        &self,
        proof: CompactBridgeZKProof,
        header: PsyBridgeHeader,
        mint_buffer_account: Pubkey,
        mint_buffer_bump: u8,
        txo_buffer_account: Pubkey,
        txo_buffer_bump: u8,
    ) -> Result<Signature, BridgeError> {
        self.process_block_transition_impl(
            proof,
            header,
            mint_buffer_account,
            mint_buffer_bump,
            txo_buffer_account,
            txo_buffer_bump,
        )
        .await
    }

    async fn process_block_reorg(
        &self,
        proof: CompactBridgeZKProof,
        header: PsyBridgeHeader,
        extra_blocks: Vec<FinalizedBlockMintTxoInfo>,
        mint_buffer_account: Pubkey,
        mint_buffer_bump: u8,
        txo_buffer_account: Pubkey,
        txo_buffer_bump: u8,
    ) -> Result<Signature, BridgeError> {
        self.process_block_reorg_impl(
            proof,
            header,
            extra_blocks,
            mint_buffer_account,
            mint_buffer_bump,
            txo_buffer_account,
            txo_buffer_bump,
        )
        .await
    }

    async fn setup_txo_buffer(
        &self,
        block_height: u32,
        txos: &[u32],
    ) -> Result<(Pubkey, u8), BridgeError> {
        self.setup_txo_buffer_impl(block_height, txos).await
    }

    async fn setup_pending_mints_buffer(
        &self,
        block_height: u32,
        pending_mints: &[PendingMint],
    ) -> Result<(Pubkey, u8), BridgeError> {
        self.setup_pending_mints_buffer_impl(block_height, pending_mints)
            .await
    }

    async fn snapshot_withdrawals(&self) -> Result<PsyWithdrawalChainSnapshot, BridgeError> {
        self.snapshot_withdrawals_impl().await
    }
}

// Implement the WithdrawalApi trait
#[async_trait]
impl WithdrawalApi for BridgeClient {
    async fn request_withdrawal(
        &self,
        user_authority: &Keypair,
        recipient_address: [u8; 20],
        amount_sats: u64,
        address_type: u32,
    ) -> Result<Signature, BridgeError> {
        self.request_withdrawal_impl(user_authority, recipient_address, amount_sats, address_type)
            .await
    }

    async fn process_withdrawal(
        &self,
        proof: CompactBridgeZKProof,
        new_return_output: PsyReturnTxOutput,
        new_spent_txo_tree_root: [u8; 32],
        new_next_processed_withdrawals_index: u64,
        doge_tx_bytes: &[u8],
    ) -> Result<Signature, BridgeError> {
        self.process_withdrawal_impl(
            proof,
            new_return_output,
            new_spent_txo_tree_root,
            new_next_processed_withdrawals_index,
            doge_tx_bytes,
        )
        .await
    }

    async fn replay_withdrawal(&self, doge_tx_bytes: &[u8]) -> Result<Signature, BridgeError> {
        self.replay_withdrawal_impl(doge_tx_bytes).await
    }
}

// Implement the ManualClaimApi trait
#[async_trait]
impl ManualClaimApi for BridgeClient {
    async fn manual_claim_deposit(
        &self,
        user_signer: &Keypair,
        proof: CompactBridgeZKProof,
        recent_block_merkle_tree_root: [u8; 32],
        recent_auto_claim_txo_root: [u8; 32],
        new_manual_claim_txo_root: [u8; 32],
        tx_hash: [u8; 32],
        combined_txo_index: u64,
        deposit_amount_sats: u64,
    ) -> Result<Signature, BridgeError> {
        self.manual_claim_deposit_impl(
            user_signer,
            proof,
            recent_block_merkle_tree_root,
            recent_auto_claim_txo_root,
            new_manual_claim_txo_root,
            tx_hash,
            combined_txo_index,
            deposit_amount_sats,
        )
        .await
    }
}

// Implement the OperatorApi trait
#[async_trait]
impl OperatorApi for BridgeClient {
    async fn initialize_bridge(
        &self,
        params: &InitializeBridgeParams,
    ) -> Result<Signature, BridgeError> {
        self.initialize_bridge_impl(params).await
    }

    async fn operator_withdraw_fees(&self) -> Result<Signature, BridgeError> {
        self.operator_withdraw_fees_impl().await
    }

    async fn execute_snapshot_withdrawals(&self) -> Result<Signature, BridgeError> {
        self.execute_snapshot_withdrawals_impl().await
    }
}

// Bridge event fetching methods
impl BridgeClient {
    /// Fetch bridge events since the given signature.
    ///
    /// Returns the last processed signature and the events in chronological order.
    /// If `after_signature` is None, fetches from the most recent transactions.
    pub async fn fetch_bridge_events_since(
        &self,
        after_signature: Option<Signature>,
        limit: Option<usize>,
    ) -> Result<(Signature, Vec<BridgeEvent>), BridgeError> {

        let monitor_config = MonitorConfig::new(
            self.config.rpc_url.clone(),
            self.config.program_id,
            self.config.bridge_state_pda,
        )
        .rate_limit(self.config.rate_limit.clone())
        .batch_size(limit.unwrap_or(100));

        let monitor = BridgeMonitor::new(monitor_config)?;
        let events = monitor.fetch_since(after_signature, limit).await?;

        // Return the last signature we processed (or default if no events)
        let last_sig = events
            .last()
            .map(|e| match e {
                crate::monitor::BridgeEvent::WithdrawalRequested(ev) => ev.signature,
                crate::monitor::BridgeEvent::WithdrawalProcessed(ev) => ev.signature,
                crate::monitor::BridgeEvent::ManualDepositClaimed(ev) => ev.signature,
                crate::monitor::BridgeEvent::BlockTransition(ev) => ev.signature,
            })
            .or(after_signature)
            .unwrap_or_default();

        Ok((last_sig, events))
    }
}
