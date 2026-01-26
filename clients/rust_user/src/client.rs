//! Main UserClient implementation.
//!
//! Provides a simple interface for end-users to interact with the Doge bridge.

use std::sync::Arc;

use psy_doge_solana_core::program_state::BridgeProgramStateWithDogeMint;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};
use spl_associated_token_account::{
    get_associated_token_address,
    instruction::create_associated_token_account,
};
use spl_token::instruction as token_instruction;

use crate::{
    config::{UserClientConfig, UserClientConfigBuilder},
    errors::{UserClientError, UserClientResult},
    instructions,
};

/// Client for end-users to interact with the Doge bridge on Solana.
///
/// This client provides simple operations for:
/// - Creating token accounts for DOGE tokens
/// - Transferring DOGE tokens
/// - Requesting withdrawals to Dogecoin
/// - Managing token account authorities
pub struct UserClient {
    config: UserClientConfig,
    rpc: Arc<RpcClient>,
    /// Cached DOGE mint address
    doge_mint_cache: tokio::sync::RwLock<Option<Pubkey>>,
}

impl UserClient {
    /// Create a new user client with just an RPC URL.
    ///
    /// Uses default program IDs for mainnet.
    pub fn new(rpc_url: &str) -> UserClientResult<Self> {
        let config = UserClientConfigBuilder::new()
            .rpc_url(rpc_url)
            .build()
            .map_err(|e| UserClientError::InvalidConfig { message: e })?;

        Self::with_config(config)
    }

    /// Create a new user client with custom configuration.
    pub fn with_config(config: UserClientConfig) -> UserClientResult<Self> {
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

    /// Create a new user client with a custom RPC URL and program ID.
    pub fn with_program_id(rpc_url: &str, program_id: Pubkey) -> UserClientResult<Self> {
        let config = UserClientConfigBuilder::new()
            .rpc_url(rpc_url)
            .program_id(program_id)
            .build()
            .map_err(|e| UserClientError::InvalidConfig { message: e })?;

        Self::with_config(config)
    }

    /// Get the bridge program ID.
    pub fn program_id(&self) -> Pubkey {
        self.config.program_id
    }

    /// Get the bridge state PDA.
    pub fn bridge_state_pda(&self) -> Pubkey {
        self.config.bridge_state_pda
    }

    /// Get the RPC client for advanced operations.
    pub fn rpc(&self) -> &RpcClient {
        &self.rpc
    }

    /// Get the DOGE mint address (cached after first fetch).
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

    /// Get the associated token account address for a user.
    pub async fn get_token_account_address(&self, owner: &Pubkey) -> UserClientResult<Pubkey> {
        let doge_mint = self.get_doge_mint().await?;
        Ok(get_associated_token_address(owner, &doge_mint))
    }

    /// Check if a token account exists for the given owner.
    pub async fn token_account_exists(&self, owner: &Pubkey) -> UserClientResult<bool> {
        let token_account = self.get_token_account_address(owner).await?;
        let account = self
            .rpc
            .get_account_with_commitment(&token_account, CommitmentConfig::confirmed())
            .await?;
        Ok(account.value.is_some())
    }

    /// Get the token balance for a user (in satoshis).
    pub async fn get_balance(&self, owner: &Pubkey) -> UserClientResult<u64> {
        let token_account = self.get_token_account_address(owner).await?;

        let account = self
            .rpc
            .get_account_with_commitment(&token_account, CommitmentConfig::confirmed())
            .await?
            .value
            .ok_or_else(|| UserClientError::AccountNotFound {
                address: token_account.to_string(),
            })?;

        let token_account_data: spl_token::state::Account =
            spl_token::state::Account::unpack(&account.data)
                .map_err(|e| UserClientError::InvalidInput(format!("Failed to parse token account: {}", e)))?;

        Ok(token_account_data.amount)
    }

    // =========================================================================
    // Core User Operations
    // =========================================================================

    /// Create an associated token account for the DOGE mint.
    ///
    /// This creates a token account owned by the user that can hold DOGE tokens.
    /// The payer pays for the account creation rent.
    ///
    /// # Arguments
    /// * `payer` - The keypair that will pay for account creation
    /// * `owner` - The public key that will own the token account (optional, defaults to payer)
    ///
    /// # Returns
    /// The transaction signature and the created token account address.
    pub async fn create_token_account(
        &self,
        payer: &Keypair,
        owner: Option<&Pubkey>,
    ) -> UserClientResult<(Signature, Pubkey)> {
        let payer_pubkey = payer.pubkey();
        let owner_pubkey = owner.unwrap_or(&payer_pubkey);
        let doge_mint = self.get_doge_mint().await?;
        let token_account = get_associated_token_address(owner_pubkey, &doge_mint);

        // Check if account already exists
        if self.token_account_exists(owner_pubkey).await? {
            return Err(UserClientError::TokenAccountExists {
                address: token_account.to_string(),
            });
        }

        let ix = create_associated_token_account(
            &payer.pubkey(),
            owner_pubkey,
            &doge_mint,
            &spl_token::id(),
        );

        let signature = self.send_and_confirm(&[ix], payer, &[]).await?;
        Ok((signature, token_account))
    }

    /// Transfer DOGE tokens to another Solana address.
    ///
    /// # Arguments
    /// * `sender` - The keypair of the sender (must own the tokens)
    /// * `recipient` - The recipient's public key
    /// * `amount_sats` - Amount to transfer in satoshis
    ///
    /// # Returns
    /// The transaction signature.
    pub async fn transfer(
        &self,
        sender: &Keypair,
        recipient: &Pubkey,
        amount_sats: u64,
    ) -> UserClientResult<Signature> {
        let doge_mint = self.get_doge_mint().await?;
        let sender_token_account = get_associated_token_address(&sender.pubkey(), &doge_mint);
        let recipient_token_account = get_associated_token_address(recipient, &doge_mint);

        // Check sender has enough balance
        let balance = self.get_balance(&sender.pubkey()).await?;
        if balance < amount_sats {
            return Err(UserClientError::InsufficientBalance {
                required: amount_sats,
                available: balance,
            });
        }

        let mut instructions = Vec::new();

        // Create recipient token account if it doesn't exist
        let recipient_exists = self
            .rpc
            .get_account_with_commitment(&recipient_token_account, CommitmentConfig::confirmed())
            .await?
            .value
            .is_some();

        if !recipient_exists {
            instructions.push(create_associated_token_account(
                &sender.pubkey(),
                recipient,
                &doge_mint,
                &spl_token::id(),
            ));
        }

        // Transfer tokens
        instructions.push(
            token_instruction::transfer(
                &spl_token::id(),
                &sender_token_account,
                &recipient_token_account,
                &sender.pubkey(),
                &[],
                amount_sats,
            )
            .map_err(|e| UserClientError::InvalidInput(format!("Failed to create transfer instruction: {}", e)))?,
        );

        self.send_and_confirm(&instructions, sender, &[]).await
    }

    /// Request a withdrawal from Solana to a Dogecoin address.
    ///
    /// This burns the DOGE tokens on Solana and queues a withdrawal request
    /// that will be processed by the bridge operator to send DOGE on the
    /// Dogecoin network.
    ///
    /// # Arguments
    /// * `user` - The keypair of the user requesting the withdrawal
    /// * `recipient_address` - 20-byte Dogecoin address (P2PKH hash160)
    /// * `amount_sats` - Amount to withdraw in satoshis
    /// * `address_type` - Address type (0 for P2PKH)
    ///
    /// # Returns
    /// The transaction signature.
    pub async fn request_withdrawal(
        &self,
        user: &Keypair,
        recipient_address: [u8; 20],
        amount_sats: u64,
        address_type: u32,
    ) -> UserClientResult<Signature> {
        let doge_mint = self.get_doge_mint().await?;
        let user_token_account = get_associated_token_address(&user.pubkey(), &doge_mint);

        // Check balance
        let balance = self.get_balance(&user.pubkey()).await?;
        if balance < amount_sats {
            return Err(UserClientError::InsufficientBalance {
                required: amount_sats,
                available: balance,
            });
        }

        let ix = instructions::request_withdrawal(
            self.config.program_id,
            user.pubkey(),
            doge_mint,
            user_token_account,
            recipient_address,
            amount_sats,
            address_type,
        );

        self.send_and_confirm(&[ix], user, &[]).await
    }

    /// Set the close authority of a token account to null.
    ///
    /// This prevents the token account from being closed, which can be useful
    /// for security purposes or to ensure the account persists.
    ///
    /// # Arguments
    /// * `owner` - The keypair of the token account owner
    /// * `token_account` - Optional specific token account (defaults to the owner's ATA)
    ///
    /// # Returns
    /// The transaction signature.
    pub async fn set_close_authority_to_null(
        &self,
        owner: &Keypair,
        token_account: Option<&Pubkey>,
    ) -> UserClientResult<Signature> {
        let doge_mint = self.get_doge_mint().await?;
        let account = token_account
            .copied()
            .unwrap_or_else(|| get_associated_token_address(&owner.pubkey(), &doge_mint));

        let ix = token_instruction::set_authority(
            &spl_token::id(),
            &account,
            None, // Set to null
            token_instruction::AuthorityType::CloseAccount,
            &owner.pubkey(),
            &[],
        )
        .map_err(|e| UserClientError::InvalidInput(format!("Failed to create set_authority instruction: {}", e)))?;

        self.send_and_confirm(&[ix], owner, &[]).await
    }

    // =========================================================================
    // Internal Helpers
    // =========================================================================

    /// Send a transaction and wait for confirmation.
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

#[cfg(test)]
mod tests {
    use crate::config::DEFAULT_BRIDGE_PROGRAM_ID;

    #[test]
    fn test_default_program_id() {
        assert_eq!(
            DEFAULT_BRIDGE_PROGRAM_ID,
            "DBjo5tqf2uwt4sg9JznSk9SBbEvsLixknN58y3trwCxJ"
        );
    }
}
