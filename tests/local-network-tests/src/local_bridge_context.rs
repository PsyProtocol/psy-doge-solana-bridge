use anyhow::{anyhow, Result};
use solana_sdk::{
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};

use crate::local_test_client::{LocalClientConfig, LocalTestClient, ProgramIds};

/// Context for running bridge tests on a local Solana network
pub struct LocalBridgeContext {
    pub client: LocalTestClient,
    pub doge_mint: Pubkey,
}

impl LocalBridgeContext {
    /// Create a new LocalBridgeContext with default configuration
    ///
    /// This will:
    /// 1. Connect to localhost:8899
    /// 2. Load program IDs from keypair files
    /// 3. Create a new payer with airdrop
    /// 4. Create a new DOGE mint controlled by the bridge PDA
    pub async fn new() -> Result<Self> {
        Self::with_config(LocalClientConfig::default()).await
    }

    /// Create a new LocalBridgeContext with custom configuration
    pub async fn with_config(config: LocalClientConfig) -> Result<Self> {
        // Load program IDs from keypairs
        let program_ids = ProgramIds::from_keypairs()?;

        // Create keypairs
        let payer = Keypair::new();
        let operator = Keypair::from_bytes(&payer.to_bytes())?;
        let fee_spender = Keypair::new();

        // Create a temporary mint keypair
        let doge_mint = Keypair::new();

        // Create client (will airdrop to payer)
        let client = LocalTestClient::new(
            config.clone(),
            payer,
            operator,
            fee_spender,
            program_ids,
            doge_mint.pubkey(),
        ).await?;

        // Verify connection
        client.health_check().await?;

        // Verify programs are deployed
        Self::verify_programs_deployed(&client).await?;

        // Create the DOGE mint
        let doge_mint_pubkey = Self::create_doge_mint(&client, &doge_mint).await?;

        Ok(Self {
            client,
            doge_mint: doge_mint_pubkey,
        })
    }

    /// Create a LocalBridgeContext reusing an existing mint
    pub async fn with_existing_mint(config: LocalClientConfig, doge_mint: Pubkey) -> Result<Self> {
        let program_ids = ProgramIds::from_keypairs()?;

        let payer = Keypair::new();
        let operator = Keypair::from_bytes(&payer.to_bytes())?;
        let fee_spender = Keypair::new();

        let client = LocalTestClient::new(
            config,
            payer,
            operator,
            fee_spender,
            program_ids,
            doge_mint,
        ).await?;

        client.health_check().await?;
        Self::verify_programs_deployed(&client).await?;

        Ok(Self { client, doge_mint })
    }

    /// Verify all required programs are deployed
    async fn verify_programs_deployed(client: &LocalTestClient) -> Result<()> {
        let programs = [
            ("doge-bridge", client.program_ids.doge_bridge),
            ("pending-mint-buffer", client.program_ids.pending_mint_buffer),
            ("txo-buffer", client.program_ids.txo_buffer),
            ("generic-buffer", client.program_ids.generic_buffer),
            ("manual-claim", client.program_ids.manual_claim),
        ];

        for (name, pubkey) in programs {
            if !client.account_exists(&pubkey).await? {
                return Err(anyhow!(
                    "Program '{}' not deployed at {}. Run 'make deploy-programs' first.",
                    name,
                    pubkey
                ));
            }
        }

        println!("All programs verified as deployed:");
        for (name, pubkey) in programs {
            println!("  {}: {}", name, pubkey);
        }

        Ok(())
    }

    /// Create the DOGE mint controlled by the bridge PDA
    async fn create_doge_mint(client: &LocalTestClient, mint_keypair: &Keypair) -> Result<Pubkey> {
        let mint_pubkey = mint_keypair.pubkey();

        // Get rent for mint account
        let rent = client.client.get_minimum_balance_for_rent_exemption(
            spl_token::state::Mint::LEN
        ).await?;

        // Create mint account
        let create_ix = system_instruction::create_account(
            &client.payer.pubkey(),
            &mint_pubkey,
            rent,
            spl_token::state::Mint::LEN as u64,
            &spl_token::id(),
        );

        // Initialize mint with bridge PDA as mint authority
        let init_ix = spl_token::instruction::initialize_mint(
            &spl_token::id(),
            &mint_pubkey,
            &client.bridge_state_pda,
            None,
            8, // decimals
        )?;

        let blockhash = client.client.get_latest_blockhash().await?;
        let tx = Transaction::new_signed_with_payer(
            &[create_ix, init_ix],
            Some(&client.payer.pubkey()),
            &[&client.payer, mint_keypair],
            blockhash,
        );

        client.client.send_and_confirm_transaction(&tx).await?;

        println!("Created DOGE mint: {}", mint_pubkey);
        println!("Bridge state PDA (mint authority): {}", client.bridge_state_pda);

        Ok(mint_pubkey)
    }

    /// Get the bridge state PDA
    pub fn bridge_state_pda(&self) -> Pubkey {
        self.client.bridge_state_pda
    }

    /// Get the current bridge state data
    pub async fn get_bridge_state_data(&self) -> Result<Vec<u8>> {
        self.client.get_account_data(&self.client.bridge_state_pda).await
    }
}

/// Builder for creating LocalBridgeContext with custom options
pub struct LocalBridgeContextBuilder {
    config: LocalClientConfig,
    payer: Option<Keypair>,
    operator: Option<Keypair>,
    fee_spender: Option<Keypair>,
    doge_mint: Option<Pubkey>,
}

impl LocalBridgeContextBuilder {
    pub fn new() -> Self {
        Self {
            config: LocalClientConfig::default(),
            payer: None,
            operator: None,
            fee_spender: None,
            doge_mint: None,
        }
    }

    pub fn with_rpc_url(mut self, url: &str) -> Self {
        self.config.rpc_url = url.to_string();
        self
    }

    pub fn with_payer(mut self, payer: Keypair) -> Self {
        self.payer = Some(payer);
        self
    }

    pub fn with_operator(mut self, operator: Keypair) -> Self {
        self.operator = Some(operator);
        self
    }

    pub fn with_fee_spender(mut self, fee_spender: Keypair) -> Self {
        self.fee_spender = Some(fee_spender);
        self
    }

    pub fn with_doge_mint(mut self, mint: Pubkey) -> Self {
        self.doge_mint = Some(mint);
        self
    }

    pub async fn build(self) -> Result<LocalBridgeContext> {
        let program_ids = ProgramIds::from_keypairs()?;

        let payer = self.payer.unwrap_or_else(Keypair::new);
        let operator = self.operator.unwrap_or_else(|| {
            Keypair::from_bytes(&payer.to_bytes()).unwrap()
        });
        let fee_spender = self.fee_spender.unwrap_or_else(|| {
            Keypair::from_bytes(&payer.to_bytes()).unwrap()
        });

        // If no mint specified, we'll create one
        let (doge_mint_pubkey, mint_keypair) = match self.doge_mint {
            Some(mint) => (mint, None),
            None => {
                let kp = Keypair::new();
                (kp.pubkey(), Some(kp))
            }
        };

        let client = LocalTestClient::new(
            self.config,
            payer,
            operator,
            fee_spender,
            program_ids,
            doge_mint_pubkey,
        ).await?;

        client.health_check().await?;
        LocalBridgeContext::verify_programs_deployed(&client).await?;

        // Create mint if needed
        if let Some(mint_kp) = mint_keypair {
            LocalBridgeContext::create_doge_mint(&client, &mint_kp).await?;
        }

        Ok(LocalBridgeContext {
            client,
            doge_mint: doge_mint_pubkey,
        })
    }
}

impl Default for LocalBridgeContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}
