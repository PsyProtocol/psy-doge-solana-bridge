//! Manual Claim Client
//!
//! A client for interacting with the manual-claim program, allowing users to:
//! - Create/derive their manual claim PDA account
//! - Submit manual claims with ZK proofs
//! - Scan transaction history for previous claims

use std::str::FromStr;
use std::sync::Arc;

use psy_doge_solana_core::instructions::manual_claim::{
    ManualClaimInstruction, MC_MANUAL_CLAIM_TRANSACTION_DESCRIMINATOR,
};
use psy_doge_solana_core::program_state::BridgeProgramStateWithDogeMint;
use psy_doge_solana_core::user_manual_deposit_manager::UserManualDepositManagerProgramState;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    system_program,
    transaction::Transaction,
};
use solana_transaction_status::UiTransactionEncoding;
use spl_associated_token_account::get_associated_token_address;

use crate::errors::{UserClientError, UserClientResult};

/// Default manual-claim program ID
pub const DEFAULT_MANUAL_CLAIM_PROGRAM_ID: &str = "MCdYbqiK3uj36tohbMjsh3Ssg8iRSJmSHToNxW8TWWE";

/// A parsed manual claim from transaction history
#[derive(Debug, Clone)]
pub struct ParsedManualClaim {
    /// Transaction signature
    pub signature: Signature,
    /// Slot the transaction was confirmed in
    pub slot: u64,
    /// Block time (Unix timestamp) if available
    pub block_time: Option<i64>,
    /// The parsed instruction data
    pub instruction: ManualClaimInstruction,
}

/// Configuration for the ManualClaimClient
#[derive(Clone)]
pub struct ManualClaimClientConfig {
    /// Solana RPC URL
    pub rpc_url: String,
    /// Manual-claim program ID
    pub manual_claim_program_id: Pubkey,
    /// Main bridge program ID
    pub bridge_program_id: Pubkey,
    /// Bridge state PDA
    pub bridge_state_pda: Pubkey,
}

/// Builder for ManualClaimClientConfig
pub struct ManualClaimClientConfigBuilder {
    rpc_url: Option<String>,
    manual_claim_program_id: Option<Pubkey>,
    bridge_program_id: Option<Pubkey>,
    bridge_state_pda: Option<Pubkey>,
}

impl ManualClaimClientConfigBuilder {
    /// Create a new config builder
    pub fn new() -> Self {
        Self {
            rpc_url: None,
            manual_claim_program_id: None,
            bridge_program_id: None,
            bridge_state_pda: None,
        }
    }

    /// Set the RPC URL
    pub fn rpc_url(mut self, url: impl Into<String>) -> Self {
        self.rpc_url = Some(url.into());
        self
    }

    /// Set the manual-claim program ID
    pub fn manual_claim_program_id(mut self, id: Pubkey) -> Self {
        self.manual_claim_program_id = Some(id);
        self
    }

    /// Set the main bridge program ID
    pub fn bridge_program_id(mut self, id: Pubkey) -> Self {
        self.bridge_program_id = Some(id);
        self
    }

    /// Set the bridge state PDA
    pub fn bridge_state_pda(mut self, pda: Pubkey) -> Self {
        self.bridge_state_pda = Some(pda);
        self
    }

    /// Build the configuration
    pub fn build(self) -> Result<ManualClaimClientConfig, String> {
        let rpc_url = self.rpc_url.ok_or("RPC URL is required")?;

        let manual_claim_program_id = self.manual_claim_program_id.unwrap_or_else(|| {
            Pubkey::from_str(DEFAULT_MANUAL_CLAIM_PROGRAM_ID).unwrap()
        });

        let bridge_program_id = self.bridge_program_id.unwrap_or_else(|| {
            Pubkey::from_str(crate::config::DEFAULT_BRIDGE_PROGRAM_ID).unwrap()
        });

        let bridge_state_pda = self.bridge_state_pda.unwrap_or_else(|| {
            Pubkey::find_program_address(&[b"bridge_state"], &bridge_program_id).0
        });

        Ok(ManualClaimClientConfig {
            rpc_url,
            manual_claim_program_id,
            bridge_program_id,
            bridge_state_pda,
        })
    }
}

impl Default for ManualClaimClientConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Client for manual claim operations
pub struct ManualClaimClient {
    config: ManualClaimClientConfig,
    rpc: Arc<RpcClient>,
    /// Cached DOGE mint address
    doge_mint_cache: tokio::sync::RwLock<Option<Pubkey>>,
}

impl ManualClaimClient {
    /// Create a new manual claim client with just an RPC URL
    pub fn new(rpc_url: &str) -> UserClientResult<Self> {
        let config = ManualClaimClientConfigBuilder::new()
            .rpc_url(rpc_url)
            .build()
            .map_err(|e| UserClientError::InvalidConfig { message: e })?;

        Self::with_config(config)
    }

    /// Create a new manual claim client with custom configuration
    pub fn with_config(config: ManualClaimClientConfig) -> UserClientResult<Self> {
        let rpc = Arc::new(RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        ));

        Ok(Self {
            config,
            rpc,
            doge_mint_cache: tokio::sync::RwLock::new(None),
        })
    }

    /// Get the RPC client for advanced operations
    pub fn rpc(&self) -> &RpcClient {
        &self.rpc
    }

    /// Get the manual-claim program ID
    pub fn manual_claim_program_id(&self) -> Pubkey {
        self.config.manual_claim_program_id
    }

    /// Get the bridge program ID
    pub fn bridge_program_id(&self) -> Pubkey {
        self.config.bridge_program_id
    }

    /// Get the bridge state PDA
    pub fn bridge_state_pda(&self) -> Pubkey {
        self.config.bridge_state_pda
    }

    /// Get the DOGE mint address (cached after first fetch)
    pub async fn get_doge_mint(&self) -> UserClientResult<Pubkey> {
        // Check cache first
        {
            let cache = self.doge_mint_cache.read().await;
            if let Some(mint) = *cache {
                return Ok(mint);
            }
        }

        // Fetch from chain
        let account = self
            .rpc
            .get_account_with_commitment(&self.config.bridge_state_pda, CommitmentConfig::confirmed())
            .await?
            .value
            .ok_or_else(|| UserClientError::AccountNotFound {
                address: self.config.bridge_state_pda.to_string(),
            })?;

        let bridge_state: &BridgeProgramStateWithDogeMint = bytemuck::from_bytes(&account.data);
        let mint = Pubkey::new_from_array(bridge_state.doge_mint);

        // Cache the result
        let mut cache = self.doge_mint_cache.write().await;
        *cache = Some(mint);

        Ok(mint)
    }

    // =========================================================================
    // PDA Derivation
    // =========================================================================

    /// Derive the manual claim PDA for a user
    ///
    /// The PDA is derived using seeds: ["manual-claim", user_pubkey]
    pub fn derive_claim_pda(&self, user: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"manual-claim", user.as_ref()],
            &self.config.manual_claim_program_id,
        )
    }

    /// Get the manual claim PDA address for a user (without bump)
    pub fn get_claim_pda_address(&self, user: &Pubkey) -> Pubkey {
        self.derive_claim_pda(user).0
    }

    /// Check if a user's manual claim PDA account exists
    pub async fn claim_account_exists(&self, user: &Pubkey) -> UserClientResult<bool> {
        let (pda, _) = self.derive_claim_pda(user);
        let account = self
            .rpc
            .get_account_with_commitment(&pda, CommitmentConfig::confirmed())
            .await?;
        Ok(account.value.is_some())
    }

    /// Get the manual claim state for a user (if it exists)
    pub async fn get_claim_state(
        &self,
        user: &Pubkey,
    ) -> UserClientResult<Option<UserManualDepositManagerProgramState>> {
        let (pda, _) = self.derive_claim_pda(user);
        let account = self
            .rpc
            .get_account_with_commitment(&pda, CommitmentConfig::confirmed())
            .await?;

        match account.value {
            Some(acc) => {
                let state: &UserManualDepositManagerProgramState =
                    bytemuck::from_bytes(&acc.data);
                Ok(Some(*state))
            }
            None => Ok(None),
        }
    }

    // =========================================================================
    // Claim Operations
    // =========================================================================

    /// Submit a manual claim
    ///
    /// This creates the claim PDA if it doesn't exist and processes the manual claim.
    ///
    /// # Arguments
    /// * `user` - The user's keypair (signer)
    /// * `payer` - The payer's keypair (pays for PDA creation if needed)
    /// * `instruction_data` - The manual claim instruction data
    ///
    /// # Returns
    /// The transaction signature
    pub async fn submit_manual_claim(
        &self,
        user: &Keypair,
        payer: &Keypair,
        instruction_data: ManualClaimInstruction,
    ) -> UserClientResult<Signature> {
        let doge_mint = self.get_doge_mint().await?;
        let (claim_pda, _) = self.derive_claim_pda(&user.pubkey());
        let recipient_ata = get_associated_token_address(&user.pubkey(), &doge_mint);

        let ix = self.build_manual_claim_instruction(
            user.pubkey(),
            payer.pubkey(),
            claim_pda,
            recipient_ata,
            doge_mint,
            instruction_data,
        );

        self.send_and_confirm(&[ix], payer, &[user]).await
    }

    /// Build a manual claim instruction
    fn build_manual_claim_instruction(
        &self,
        user: Pubkey,
        payer: Pubkey,
        claim_pda: Pubkey,
        recipient_ata: Pubkey,
        doge_mint: Pubkey,
        instruction_data: ManualClaimInstruction,
    ) -> Instruction {
        let data = gen_aligned_instruction(
            MC_MANUAL_CLAIM_TRANSACTION_DESCRIMINATOR,
            bytemuck::bytes_of(&instruction_data),
        );

        Instruction {
            program_id: self.config.manual_claim_program_id,
            accounts: vec![
                AccountMeta::new(claim_pda, false),              // claim_state_pda (writable)
                AccountMeta::new_readonly(self.config.bridge_state_pda, false), // bridge_state_account
                AccountMeta::new(recipient_ata, false),          // recipient_account (writable)
                AccountMeta::new(doge_mint, false),              // doge_mint (writable)
                AccountMeta::new_readonly(spl_token::id(), false), // token_program
                AccountMeta::new_readonly(self.config.bridge_program_id, false), // main_bridge_program
                AccountMeta::new_readonly(user, true),           // user (signer)
                AccountMeta::new(payer, true),                   // payer (signer, writable)
                AccountMeta::new_readonly(system_program::id(), false), // system_program
            ],
            data,
        }
    }

    // =========================================================================
    // History Scanning
    // =========================================================================

    /// Get all manual claims for a user by scanning transaction history
    ///
    /// This fetches signatures for the user's manual claim PDA and parses the
    /// ManualClaimInstruction from each transaction.
    ///
    /// # Arguments
    /// * `user` - The user's public key
    /// * `before` - Optional signature to start scanning before (for pagination)
    /// * `limit` - Maximum number of claims to return (default: 100)
    ///
    /// # Returns
    /// A vector of parsed manual claims
    pub async fn get_claim_history(
        &self,
        user: &Pubkey,
        before: Option<Signature>,
        limit: Option<usize>,
    ) -> UserClientResult<Vec<ParsedManualClaim>> {
        let (claim_pda, _) = self.derive_claim_pda(user);
        let limit = limit.unwrap_or(100);

        // Fetch signatures for the PDA
        let signatures = self
            .rpc
            .get_signatures_for_address_with_config(
                &claim_pda,
                GetConfirmedSignaturesForAddress2Config {
                    before,
                    until: None,
                    limit: Some(limit),
                    commitment: Some(CommitmentConfig::confirmed()),
                },
            )
            .await?;

        let mut claims = Vec::new();

        for sig_info in signatures {
            // Skip failed transactions
            if sig_info.err.is_some() {
                continue;
            }

            let signature = Signature::from_str(&sig_info.signature)
                .map_err(|e| UserClientError::InvalidInput(format!("Invalid signature: {}", e)))?;

            // Fetch the full transaction
            match self.fetch_and_parse_claim_transaction(&signature).await {
                Ok(Some(instruction)) => {
                    claims.push(ParsedManualClaim {
                        signature,
                        slot: sig_info.slot,
                        block_time: sig_info.block_time,
                        instruction,
                    });
                }
                Ok(None) => {
                    // Transaction didn't contain a manual claim instruction
                    continue;
                }
                Err(_) => {
                    // Skip transactions we can't parse
                    continue;
                }
            }
        }

        Ok(claims)
    }

    /// Fetch and parse a manual claim instruction from a transaction
    async fn fetch_and_parse_claim_transaction(
        &self,
        signature: &Signature,
    ) -> UserClientResult<Option<ManualClaimInstruction>> {
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

        // Parse the transaction to extract the manual claim instruction
        if let Some(transaction) = tx.transaction.transaction.decode() {
            let message = transaction.message;

            for ix in message.instructions().iter() {
                let program_id_index = ix.program_id_index as usize;
                if program_id_index >= message.static_account_keys().len() {
                    continue;
                }

                let program_id = message.static_account_keys()[program_id_index];

                if program_id == self.config.manual_claim_program_id {
                    let data = ix.data.as_slice();

                    // Check discriminator (first byte repeated 8 times)
                    if data.len() >= 8 && data[0] == MC_MANUAL_CLAIM_TRANSACTION_DESCRIMINATOR {
                        // Parse the instruction data
                        if data.len() >= 8 + std::mem::size_of::<ManualClaimInstruction>() {
                            let instruction: &ManualClaimInstruction =
                                bytemuck::from_bytes(&data[8..8 + std::mem::size_of::<ManualClaimInstruction>()]);
                            return Ok(Some(*instruction));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Get all claim history without pagination (fetches all pages)
    ///
    /// This repeatedly calls get_claim_history until all claims are fetched.
    /// Use with caution for users with many claims.
    ///
    /// # Arguments
    /// * `user` - The user's public key
    /// * `batch_size` - Number of claims to fetch per request (default: 100)
    ///
    /// # Returns
    /// A vector of all parsed manual claims
    pub async fn get_all_claim_history(
        &self,
        user: &Pubkey,
        batch_size: Option<usize>,
    ) -> UserClientResult<Vec<ParsedManualClaim>> {
        let batch_size = batch_size.unwrap_or(100);
        let mut all_claims = Vec::new();
        let mut before: Option<Signature> = None;

        loop {
            let claims = self.get_claim_history(user, before, Some(batch_size)).await?;

            if claims.is_empty() {
                break;
            }

            // Set the "before" cursor to the last signature for next page
            before = claims.last().map(|c| c.signature);

            let fetched_count = claims.len();
            all_claims.extend(claims);

            // If we got fewer than batch_size, we've reached the end
            if fetched_count < batch_size {
                break;
            }
        }

        Ok(all_claims)
    }

    // =========================================================================
    // Internal Helpers
    // =========================================================================

    /// Send a transaction and wait for confirmation
    async fn send_and_confirm(
        &self,
        instructions: &[Instruction],
        payer: &Keypair,
        extra_signers: &[&Keypair],
    ) -> UserClientResult<Signature> {
        let recent_blockhash = self.rpc.get_latest_blockhash().await?;

        let mut signers: Vec<&Keypair> = vec![payer];
        signers.extend(extra_signers);

        let tx = Transaction::new_signed_with_payer(
            instructions,
            Some(&payer.pubkey()),
            &signers,
            recent_blockhash,
        );

        let signature = self.rpc.send_and_confirm_transaction(&tx).await?;
        Ok(signature)
    }
}

/// Generate aligned instruction data with the discriminator
fn gen_aligned_instruction(instruction_discriminator: u8, data_struct_bytes: &[u8]) -> Vec<u8> {
    let mut data = vec![instruction_discriminator; 8];
    data.extend_from_slice(data_struct_bytes);
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_program_id() {
        assert_eq!(
            DEFAULT_MANUAL_CLAIM_PROGRAM_ID,
            "MCdYbqiK3uj36tohbMjsh3Ssg8iRSJmSHToNxW8TWWE"
        );
    }

    #[test]
    fn test_pda_derivation() {
        let config = ManualClaimClientConfigBuilder::new()
            .rpc_url("http://localhost:8899")
            .build()
            .unwrap();

        let client = ManualClaimClient::with_config(config).unwrap();
        let user = Pubkey::new_unique();

        let (pda, bump) = client.derive_claim_pda(&user);

        // Verify the PDA is derived correctly
        let expected = Pubkey::find_program_address(
            &[b"manual-claim", user.as_ref()],
            &Pubkey::from_str(DEFAULT_MANUAL_CLAIM_PROGRAM_ID).unwrap(),
        );

        assert_eq!(pda, expected.0);
        assert_eq!(bump, expected.1);
    }

    #[test]
    fn test_gen_aligned_instruction() {
        let discriminator = 0u8;
        let data = vec![1, 2, 3, 4];

        let result = gen_aligned_instruction(discriminator, &data);

        // First 8 bytes should be the discriminator
        assert_eq!(result[0..8], [0, 0, 0, 0, 0, 0, 0, 0]);
        // Remaining bytes should be the data
        assert_eq!(&result[8..], &[1, 2, 3, 4]);
    }
}
