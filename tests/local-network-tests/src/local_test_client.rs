use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::RpcSendTransactionConfig,
};
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    instruction::Instruction,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    system_instruction,
    transaction::Transaction,
};
use spl_token::instruction::{set_authority, AuthorityType};

use psy_bridge_core::{
    crypto::{hash::sha256_impl::hash_impl_sha256_bytes, zk::CompactBridgeZKProof},
    header::PsyBridgeHeader,
};
use psy_doge_solana_core::{
    data_accounts::pending_mint::{
        PendingMint, PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH, PM_DA_PENDING_MINT_SIZE,
        PM_MAX_PENDING_MINTS_PER_GROUP, PM_TXO_DEFAULT_BUFFER_HASH,
    },
    instructions::doge_bridge::InitializeBridgeParams,
    program_state::FinalizedBlockMintTxoInfo,
};

use doge_bridge_client::instructions;

const CHUNK_SIZE: usize = 900;
const MAX_RETRIES: u32 = 10;
const RETRY_DELAY_MS: u64 = 500;
const CONFIRMATION_TIMEOUT_SECS: u64 = 60;

/// Configuration for the local test client
#[derive(Clone)]
pub struct LocalClientConfig {
    pub rpc_url: String,
    pub commitment: CommitmentConfig,
    pub skip_preflight: bool,
    pub airdrop_amount: u64,
}

impl Default for LocalClientConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://localhost:8899".to_string(),
            commitment: CommitmentConfig::confirmed(),
            skip_preflight: true,
            airdrop_amount: 100_000_000_000, // 100 SOL
        }
    }
}

/// Program IDs configuration - allows using either hardcoded or custom program IDs
#[derive(Clone)]
pub struct ProgramIds {
    pub doge_bridge: Pubkey,
    pub manual_claim: Pubkey,
    pub pending_mint_buffer: Pubkey,
    pub txo_buffer: Pubkey,
    pub generic_buffer: Pubkey,
}

impl ProgramIds {
    /// Load program IDs from keypair files in the program-keys directory
    pub fn from_keypairs() -> Result<Self> {
        Ok(Self {
            doge_bridge: crate::get_program_id("doge-bridge")?,
            manual_claim: crate::get_program_id("manual-claim")?,
            pending_mint_buffer: crate::get_program_id("pending-mint")?,
            txo_buffer: crate::get_program_id("txo-buffer")?,
            generic_buffer: crate::get_program_id("generic-buffer")?,
        })
    }
}

/// Transaction result with signature and optional logs
#[derive(Debug)]
pub struct TxResult {
    pub signature: Signature,
    pub slot: u64,
}

/// Comprehensive client for interacting with the Doge Bridge on a local Solana network
pub struct LocalTestClient {
    pub client: Arc<RpcClient>,
    pub config: LocalClientConfig,

    // Keypairs
    pub payer: Keypair,
    pub operator: Keypair,
    pub fee_spender: Keypair,

    // Program IDs
    pub program_ids: ProgramIds,

    // Bridge state
    pub bridge_state_pda: Pubkey,
    pub doge_mint: Pubkey,

    // Wormhole (optional, for withdrawal testing)
    pub wormhole_core_program_id: Option<Pubkey>,
    pub wormhole_shim_program_id: Option<Pubkey>,

    // Tracking
    current_txo_batch_id: u32,
}

impl LocalTestClient {
    /// Create a new LocalTestClient with the given configuration
    pub async fn new(
        config: LocalClientConfig,
        payer: Keypair,
        operator: Keypair,
        fee_spender: Keypair,
        program_ids: ProgramIds,
        doge_mint: Pubkey,
    ) -> Result<Self> {
        let client = Arc::new(RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            config.commitment,
        ));

        let (bridge_state_pda, _) = Pubkey::find_program_address(
            &[b"bridge_state"],
            &program_ids.doge_bridge,
        );

        let this = Self {
            client,
            config,
            payer,
            operator,
            fee_spender,
            program_ids,
            bridge_state_pda,
            doge_mint,
            wormhole_core_program_id: None,
            wormhole_shim_program_id: None,
            current_txo_batch_id: 0,
        };

        // Ensure payer has sufficient balance
        this.ensure_balance(&this.payer.pubkey(), this.config.airdrop_amount).await?;

        Ok(this)
    }

    /// Clone the client (creates new keypair copies)
    pub fn try_clone(&self) -> Result<Self> {
        Ok(Self {
            client: self.client.clone(),
            config: self.config.clone(),
            payer: Keypair::from_bytes(&self.payer.to_bytes())?,
            operator: Keypair::from_bytes(&self.operator.to_bytes())?,
            fee_spender: Keypair::from_bytes(&self.fee_spender.to_bytes())?,
            program_ids: self.program_ids.clone(),
            bridge_state_pda: self.bridge_state_pda,
            doge_mint: self.doge_mint,
            wormhole_core_program_id: self.wormhole_core_program_id,
            wormhole_shim_program_id: self.wormhole_shim_program_id,
            current_txo_batch_id: self.current_txo_batch_id,
        })
    }

    // =========================================================================
    // Connection & Health
    // =========================================================================

    /// Check if the validator is running and reachable
    pub async fn health_check(&self) -> Result<()> {
        self.client.get_health().await
            .map_err(|e| anyhow!("Validator health check failed: {}. Is the validator running?", e))
    }

    /// Get the current slot
    pub async fn get_slot(&self) -> Result<u64> {
        Ok(self.client.get_slot().await?)
    }

    /// Wait for a new slot
    pub async fn wait_for_new_slot(&self, current_slot: u64) -> Result<u64> {
        let start = std::time::Instant::now();
        loop {
            let slot = self.get_slot().await?;
            if slot > current_slot {
                return Ok(slot);
            }
            if start.elapsed() > Duration::from_secs(30) {
                return Err(anyhow!("Timeout waiting for new slot"));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    // =========================================================================
    // Balance & Airdrop
    // =========================================================================

    /// Get the balance of an account in lamports
    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        Ok(self.client.get_balance(pubkey).await?)
    }

    /// Request an airdrop for the given account
    pub async fn airdrop(&self, pubkey: &Pubkey, lamports: u64) -> Result<Signature> {
        let sig = self.client.request_airdrop(pubkey, lamports).await?;
        self.wait_for_confirmation(&sig).await?;
        Ok(sig)
    }

    /// Ensure an account has at least the specified balance
    pub async fn ensure_balance(&self, pubkey: &Pubkey, min_lamports: u64) -> Result<()> {
        let balance = self.get_balance(pubkey).await?;
        if balance < min_lamports {
            let needed = min_lamports - balance;
            println!("Requesting airdrop of {} lamports for {}", needed, pubkey);
            self.airdrop(pubkey, needed).await?;
        }
        Ok(())
    }

    // =========================================================================
    // Transaction Sending & Confirmation
    // =========================================================================

    /// Send a transaction and wait for confirmation
    pub async fn send_tx(
        &self,
        instructions: &[Instruction],
        extra_signers: &[&Keypair],
    ) -> Result<TxResult> {
        self.send_tx_with_config(instructions, extra_signers, self.config.skip_preflight).await
    }

    /// Send a transaction with custom configuration
    pub async fn send_tx_with_config(
        &self,
        instructions: &[Instruction],
        extra_signers: &[&Keypair],
        skip_preflight: bool,
    ) -> Result<TxResult> {
        let blockhash = self.client.get_latest_blockhash().await?;

        let mut signers: Vec<&Keypair> = vec![&self.payer];
        signers.extend(extra_signers);

        let tx = Transaction::new_signed_with_payer(
            instructions,
            Some(&self.payer.pubkey()),
            &signers,
            blockhash,
        );

        let config = RpcSendTransactionConfig {
            skip_preflight,
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            ..Default::default()
        };

        let signature = self.send_with_retry(&tx, &config).await?;
        let slot = self.wait_for_confirmation(&signature).await?;

        Ok(TxResult { signature, slot })
    }

    /// Send transaction with retry logic
    async fn send_with_retry(
        &self,
        tx: &Transaction,
        config: &RpcSendTransactionConfig,
    ) -> Result<Signature> {
        let mut last_error = None;

        for attempt in 0..MAX_RETRIES {
            match self.client.send_transaction_with_config(tx, *config).await {
                Ok(sig) => return Ok(sig),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }

        Err(anyhow!("Failed to send transaction after {} retries: {:?}", MAX_RETRIES, last_error))
    }

    /// Wait for transaction confirmation
    pub async fn wait_for_confirmation(&self, signature: &Signature) -> Result<u64> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(CONFIRMATION_TIMEOUT_SECS);

        loop {
            if start.elapsed() > timeout {
                return Err(anyhow!("Transaction confirmation timeout for {}", signature));
            }

            match self.client.get_signature_status(signature).await? {
                Some(Ok(())) => {
                    // Get the slot where the transaction was confirmed
                    if let Ok(status) = self.client.get_signature_statuses(&[*signature]).await {
                        if let Some(Some(status)) = status.value.first() {
                            return Ok(status.slot);
                        }
                    }
                    return Ok(0); // Fallback if we can't get the slot
                }
                Some(Err(e)) => {
                    return Err(anyhow!("Transaction failed: {:?}", e));
                }
                None => {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        }
    }

    // =========================================================================
    // Account Reading
    // =========================================================================

    /// Get account data
    pub async fn get_account_data(&self, pubkey: &Pubkey) -> Result<Vec<u8>> {
        let account = self.client.get_account(pubkey).await
            .map_err(|e| anyhow!("Failed to get account {}: {}", pubkey, e))?;
        Ok(account.data)
    }

    /// Check if an account exists
    pub async fn account_exists(&self, pubkey: &Pubkey) -> Result<bool> {
        match self.client.get_account_with_commitment(pubkey, self.config.commitment).await? {
            response => Ok(response.value.is_some()),
        }
    }

    // =========================================================================
    // Token Operations
    // =========================================================================

    /// Create or get an Associated Token Account for the given mint and owner
    pub async fn create_token_ata_if_needed(
        &self,
        mint: &Pubkey,
        owner: &Keypair,
    ) -> Result<(Pubkey, u64)> {
        let ata = spl_associated_token_account::get_associated_token_address(
            &owner.pubkey(),
            mint,
        );

        if let Ok(account) = self.client.get_account(&ata).await {
            let token_account = spl_token::state::Account::unpack(&account.data)?;
            return Ok((ata, token_account.amount));
        }

        // Create the ATA
        let create_ix = spl_associated_token_account::instruction::create_associated_token_account(
            &self.payer.pubkey(),
            &owner.pubkey(),
            mint,
            &spl_token::id(),
        );
        self.send_tx(&[create_ix], &[]).await?;

        // Remove close authority (match original behavior)
        let null_closer_ix = set_authority(
            &spl_token::id(),
            &ata,
            None,
            AuthorityType::CloseAccount,
            &owner.pubkey(),
            &[],
        )?;
        self.send_tx(&[null_closer_ix], &[owner]).await?;

        Ok((ata, 0))
    }

    /// Get token balance for an ATA
    pub async fn get_token_balance(&self, ata: &Pubkey) -> Result<u64> {
        let account = self.client.get_account(ata).await?;
        let token_account = spl_token::state::Account::unpack(&account.data)?;
        Ok(token_account.amount)
    }

    // =========================================================================
    // Bridge Operations
    // =========================================================================

    /// Initialize the bridge
    pub async fn initialize_bridge(&self, params: &InitializeBridgeParams) -> Result<TxResult> {
        let ix = instructions::initialize_bridge(
            self.payer.pubkey(),
            self.operator.pubkey(),
            self.fee_spender.pubkey(),
            self.doge_mint,
            params,
        );
        self.send_tx(&[ix], &[]).await
    }

    /// Send a block update transaction
    pub async fn send_block_update(
        &self,
        proof: CompactBridgeZKProof,
        header: PsyBridgeHeader,
        mint_buffer: Pubkey,
        txo_buffer: Pubkey,
        mint_buffer_bump: u8,
        txo_buffer_bump: u8,
    ) -> Result<TxResult> {
        let ix = instructions::block_update(
            self.program_ids.doge_bridge,
            self.payer.pubkey(),
            proof,
            header,
            self.operator.pubkey(),
            mint_buffer,
            txo_buffer,
            mint_buffer_bump,
            txo_buffer_bump,
        );
        self.send_tx(&[ix], &[&self.operator]).await
    }

    /// Send a reorg blocks transaction
    pub async fn send_reorg_blocks(
        &self,
        proof: CompactBridgeZKProof,
        header: PsyBridgeHeader,
        extra_blocks: Vec<FinalizedBlockMintTxoInfo>,
        mint_buffer: Pubkey,
        txo_buffer: Pubkey,
        mint_buffer_bump: u8,
        txo_buffer_bump: u8,
    ) -> Result<TxResult> {
        let ix = instructions::process_reorg_blocks(
            self.program_ids.doge_bridge,
            self.payer.pubkey(),
            proof,
            header,
            extra_blocks,
            self.operator.pubkey(),
            mint_buffer,
            txo_buffer,
            mint_buffer_bump,
            txo_buffer_bump,
        );
        self.send_tx(&[ix], &[&self.operator]).await
    }

    /// Process a mint group
    pub async fn process_mint_group(
        &self,
        mint_buffer: Pubkey,
        recipients: Vec<Pubkey>,
        group_index: u16,
        mint_buffer_bump: u8,
        should_unlock: bool,
    ) -> Result<TxResult> {
        let ix = instructions::process_mint_group(
            self.program_ids.doge_bridge,
            self.operator.pubkey(),
            mint_buffer,
            self.doge_mint,
            recipients,
            group_index,
            mint_buffer_bump,
            should_unlock,
        );
        self.send_tx(&[ix], &[&self.operator]).await
    }

    /// Process a mint group with auto-advance
    pub async fn process_mint_group_auto_advance(
        &self,
        mint_buffer: Pubkey,
        txo_buffer: Pubkey,
        recipients: Vec<Pubkey>,
        group_index: u16,
        mint_buffer_bump: u8,
        txo_buffer_bump: u8,
        should_unlock: bool,
    ) -> Result<TxResult> {
        let ix = instructions::process_mint_group_auto_advance(
            self.program_ids.doge_bridge,
            self.operator.pubkey(),
            mint_buffer,
            txo_buffer,
            self.doge_mint,
            recipients,
            group_index,
            mint_buffer_bump,
            txo_buffer_bump,
            should_unlock,
        );
        self.send_tx(&[ix], &[&self.operator]).await
    }

    // =========================================================================
    // Buffer Operations
    // =========================================================================

    /// Create a generic buffer
    pub async fn create_generic_buffer(&self, data: &[u8]) -> Result<Pubkey> {
        let buffer_account = Keypair::new();
        let buffer_pubkey = buffer_account.pubkey();
        let target_size = data.len() as u32;

        let space = 32;
        let rent = self.client.get_minimum_balance_for_rent_exemption(space).await?;

        let create_ix = system_instruction::create_account(
            &self.payer.pubkey(),
            &buffer_pubkey,
            rent,
            space as u64,
            &self.program_ids.generic_buffer,
        );
        let init_ix = instructions::generic_buffer_init(
            self.program_ids.generic_buffer,
            buffer_pubkey,
            self.payer.pubkey(),
            target_size,
        );

        self.send_tx(&[create_ix, init_ix], &[&buffer_account]).await?;

        // Write data in chunks
        for (i, chunk) in data.chunks(CHUNK_SIZE).enumerate() {
            let offset = (i * CHUNK_SIZE) as u32;
            let write_ix = instructions::generic_buffer_write(
                self.program_ids.generic_buffer,
                buffer_pubkey,
                self.payer.pubkey(),
                offset,
                chunk,
            );
            self.send_tx(&[write_ix], &[]).await?;
        }

        Ok(buffer_pubkey)
    }

    /// Create or reinitialize a pending mint buffer
    pub async fn create_pending_mint_buffer(
        &mut self,
        locker: Pubkey,
        mints: &[PendingMint],
    ) -> Result<(Pubkey, u8)> {
        let operator_pubkey = self.operator.pubkey().to_bytes();
        let seeds: &[&[u8]] = &[b"mint_buffer", &operator_pubkey];
        let (buffer_pubkey, bump) = Pubkey::find_program_address(
            seeds,
            &self.program_ids.pending_mint_buffer,
        );

        // Check if account exists
        let exists = self.account_exists(&buffer_pubkey).await?;

        if !exists {
            let space = 72;
            let rent = self.client.get_minimum_balance_for_rent_exemption(space).await?;

            let transfer_ix = system_instruction::transfer(
                &self.payer.pubkey(),
                &buffer_pubkey,
                rent,
            );
            let setup_ix = instructions::pending_mint_setup(
                self.program_ids.pending_mint_buffer,
                buffer_pubkey,
                locker,
                self.operator.pubkey(),
            );
            self.send_tx(&[transfer_ix, setup_ix], &[]).await?;
        }

        // Reinitialize with new mint count
        let total_mints = mints.len() as u16;
        let reinit_ix = instructions::pending_mint_reinit(
            self.program_ids.pending_mint_buffer,
            buffer_pubkey,
            self.operator.pubkey(),
            total_mints,
        );
        self.send_tx(&[reinit_ix], &[&self.operator]).await?;

        // Insert mints in groups
        let groups_count = (mints.len() + PM_MAX_PENDING_MINTS_PER_GROUP - 1)
            / PM_MAX_PENDING_MINTS_PER_GROUP;

        for group_idx in 0..groups_count {
            let start = group_idx * PM_MAX_PENDING_MINTS_PER_GROUP;
            let end = std::cmp::min(start + PM_MAX_PENDING_MINTS_PER_GROUP, mints.len());
            let group_mints = &mints[start..end];

            let mut mint_data = Vec::with_capacity(group_mints.len() * PM_DA_PENDING_MINT_SIZE);
            for m in group_mints {
                mint_data.extend_from_slice(bytemuck::bytes_of(m));
            }

            let insert_ix = instructions::pending_mint_insert(
                self.program_ids.pending_mint_buffer,
                buffer_pubkey,
                self.operator.pubkey(),
                group_idx as u16,
                &mint_data,
            );
            self.send_tx(&[insert_ix], &[&self.operator]).await?;
        }

        Ok((buffer_pubkey, bump))
    }

    /// Create or reinitialize a TXO buffer
    pub async fn create_txo_buffer(
        &mut self,
        doge_block_height: u32,
        txo_indices: &[u32],
    ) -> Result<(Pubkey, u8)> {
        let operator_pubkey = self.operator.pubkey().to_bytes();
        let seeds: &[&[u8]] = &[b"txo_buffer", &operator_pubkey];
        let (buffer_pubkey, bump) = Pubkey::find_program_address(
            seeds,
            &self.program_ids.txo_buffer,
        );

        let txo_bytes: Vec<u8> = txo_indices.iter().flat_map(|x| x.to_le_bytes()).collect();
        let total_len = txo_bytes.len() as u32;

        // Check if account exists and get current batch_id
        let (exists, batch_id) = match self.client.get_account_with_commitment(
            &buffer_pubkey,
            self.config.commitment,
        ).await?.value {
            Some(account) => {
                let current_batch_id = u32::from_le_bytes(
                    account.data[40..44].try_into().unwrap_or([0; 4])
                );
                (true, current_batch_id + 1)
            }
            None => (false, 0),
        };

        if !exists {
            let space = 48;
            let rent = self.client.get_minimum_balance_for_rent_exemption(space).await?;

            let transfer_ix = system_instruction::transfer(
                &self.payer.pubkey(),
                &buffer_pubkey,
                rent,
            );
            let init_ix = instructions::txo_buffer_init(
                self.program_ids.txo_buffer,
                buffer_pubkey,
                self.operator.pubkey(),
            );
            self.send_tx(&[transfer_ix, init_ix], &[]).await?;
        }

        self.current_txo_batch_id = batch_id;

        // Set length and prepare for writing
        let set_len_ix = instructions::txo_buffer_set_len(
            self.program_ids.txo_buffer,
            buffer_pubkey,
            self.payer.pubkey(),
            self.operator.pubkey(),
            total_len,
            true,  // resize
            batch_id,
            doge_block_height,
            false, // not finalizing yet
        );
        self.send_tx(&[set_len_ix], &[&self.operator]).await?;

        // Write data in chunks
        for (i, chunk) in txo_bytes.chunks(CHUNK_SIZE).enumerate() {
            let offset = (i * CHUNK_SIZE) as u32;
            let write_ix = instructions::txo_buffer_write(
                self.program_ids.txo_buffer,
                buffer_pubkey,
                self.operator.pubkey(),
                batch_id,
                offset,
                chunk,
            );
            self.send_tx(&[write_ix], &[&self.operator]).await?;
        }

        // Finalize
        let finalize_ix = instructions::txo_buffer_set_len(
            self.program_ids.txo_buffer,
            buffer_pubkey,
            self.payer.pubkey(),
            self.operator.pubkey(),
            total_len,
            false, // no resize
            batch_id,
            doge_block_height,
            true,  // finalize
        );
        self.send_tx(&[finalize_ix], &[&self.operator]).await?;

        Ok((buffer_pubkey, bump))
    }

    // =========================================================================
    // PDA Helpers
    // =========================================================================

    /// Get mint buffer PDA and bump (derived from operator key)
    pub fn get_mint_buffer_pda(&self) -> (Pubkey, u8) {
        let operator_pubkey = self.operator.pubkey().to_bytes();
        Pubkey::find_program_address(
            &[b"mint_buffer", &operator_pubkey],
            &self.program_ids.pending_mint_buffer,
        )
    }

    /// Get TXO buffer PDA and bump (derived from operator key)
    pub fn get_txo_buffer_pda(&self) -> (Pubkey, u8) {
        let operator_pubkey = self.operator.pubkey().to_bytes();
        Pubkey::find_program_address(
            &[b"txo_buffer", &operator_pubkey],
            &self.program_ids.txo_buffer,
        )
    }

    // =========================================================================
    // Hash Computation Helpers
    // =========================================================================

    /// Compute the hash of pending mints
    pub fn compute_mints_hash(mints: &[PendingMint]) -> [u8; 32] {
        if mints.is_empty() {
            return PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH;
        }

        let count = mints.len() as u16;
        let group_size = PM_MAX_PENDING_MINTS_PER_GROUP;
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

    /// Compute the hash of TXO indices
    pub fn compute_txo_hash(indices: &[u32]) -> [u8; 32] {
        if indices.is_empty() {
            return PM_TXO_DEFAULT_BUFFER_HASH;
        }
        let bytes: Vec<u8> = indices.iter().flat_map(|x| x.to_le_bytes()).collect();
        hash_impl_sha256_bytes(&bytes)
    }
}
