use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::Args;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signer},
    transaction::Transaction,
};
use spl_associated_token_account::{
    get_associated_token_address,
    instruction::create_associated_token_account,
};

/// Output structure for user account info
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct UserAccountInfo {
    pub pubkey: String,
    pub pubkey_hex: String,
    pub private_key: Vec<u8>,
    pub doge_ata: String,
    pub doge_ata_hex: String,
}

#[derive(Args)]
pub struct CreateUserArgs {
    /// DOGE token mint address
    #[arg(long, short = 'm')]
    doge_mint: String,

    /// Output path to save user account JSON
    #[arg(long, short = 'o', default_value = "./bridge-config/users/user.json")]
    output: PathBuf,

    /// Path to existing user keypair (if provided, will use this instead of generating new)
    #[arg(long)]
    user_keypair: Option<PathBuf>,

    /// Skip creating the ATA on-chain (just output the derived address)
    #[arg(long)]
    skip_ata_creation: bool,
}

pub fn execute(rpc_url: &str, payer_keypair_path: Option<PathBuf>, args: CreateUserArgs) -> Result<()> {
    let doge_mint: Pubkey = args.doge_mint.parse()
        .with_context(|| format!("Invalid DOGE mint address: {}", args.doge_mint))?;

    // Load or create user keypair
    let (user_keypair, is_new) = if let Some(user_path) = &args.user_keypair {
        let keypair = read_keypair_file(user_path)
            .map_err(|e| anyhow!("Failed to read user keypair from {:?}: {}", user_path, e))?;
        println!("Loaded existing user keypair from {:?}", user_path);
        (keypair, false)
    } else if args.output.exists() {
        // Try to load from output path if it exists
        let content = fs::read_to_string(&args.output)
            .with_context(|| format!("Failed to read existing user file: {:?}", args.output))?;
        let info: UserAccountInfo = serde_json::from_str(&content)
            .with_context(|| "Failed to parse existing user file")?;
        let keypair = Keypair::from_bytes(&info.private_key)
            .map_err(|e| anyhow!("Failed to reconstruct keypair from file: {}", e))?;
        println!("Loaded existing user from {:?}", args.output);
        (keypair, false)
    } else {
        let keypair = Keypair::new();
        println!("Generated new user keypair");
        (keypair, true)
    };

    let user_pubkey = user_keypair.pubkey();
    println!("\nUser Account:");
    println!("  Pubkey (base58): {}", user_pubkey);
    println!("  Pubkey (hex):    {}", hex::encode(user_pubkey.to_bytes()));

    // Derive the associated token account address
    let doge_ata = get_associated_token_address(&user_pubkey, &doge_mint);
    println!("\nDOGE Token Account (ATA):");
    println!("  Address (base58): {}", doge_ata);
    println!("  Address (hex):    {}", hex::encode(doge_ata.to_bytes()));
    println!("  DOGE Mint: {}", doge_mint);

    // Create ATA on-chain if requested
    if !args.skip_ata_creation {
        let client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

        // Check if ATA already exists
        let ata_exists = client.get_account(&doge_ata).is_ok();

        if ata_exists {
            println!("\n  ATA already exists on-chain");
        } else {
            // Load payer keypair
            let payer = load_payer_keypair(payer_keypair_path)?;
            println!("\nCreating ATA on-chain...");
            println!("  Payer: {}", payer.pubkey());

            let create_ata_ix = create_associated_token_account(
                &payer.pubkey(),
                &user_pubkey,
                &doge_mint,
                &spl_token::id(),
            );

            let recent_blockhash = client
                .get_latest_blockhash()
                .context("Failed to get latest blockhash")?;

            let transaction = Transaction::new_signed_with_payer(
                &[create_ata_ix],
                Some(&payer.pubkey()),
                &[&payer],
                recent_blockhash,
            );

            let signature = client
                .send_and_confirm_transaction(&transaction)
                .context("Failed to create ATA")?;

            println!("  ATA created: {}", signature);
        }
    } else {
        println!("\n  Skipping ATA creation (--skip-ata-creation)");
    }

    // Save user account info
    let user_info = UserAccountInfo {
        pubkey: user_pubkey.to_string(),
        pubkey_hex: hex::encode(user_pubkey.to_bytes()),
        private_key: user_keypair.to_bytes().to_vec(),
        doge_ata: doge_ata.to_string(),
        doge_ata_hex: hex::encode(doge_ata.to_bytes()),
    };

    // Only write if new keypair or file doesn't exist
    if is_new || !args.output.exists() {
        let json = serde_json::to_string_pretty(&user_info)?;

        // Create parent directories if needed
        if let Some(parent) = args.output.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }

        fs::write(&args.output, json)
            .with_context(|| format!("Failed to write user info to {:?}", args.output))?;
        println!("\nUser info saved to: {:?}", args.output);
    }

    println!("\n========== USER ACCOUNT CREATED ==========");
    println!("Pubkey:       {}", user_pubkey);
    println!("Pubkey (hex): {}", hex::encode(user_pubkey.to_bytes()));
    println!("DOGE ATA:     {}", doge_ata);
    println!("ATA (hex):    {}", hex::encode(doge_ata.to_bytes()));
    println!("==========================================");

    Ok(())
}

fn load_payer_keypair(keypair_path: Option<PathBuf>) -> Result<Keypair> {
    match keypair_path {
        Some(path) => read_keypair_file(&path)
            .map_err(|e| anyhow!("Failed to read payer keypair from {:?}: {}", path, e)),
        None => {
            let default_path = dirs::home_dir()
                .map(|h| h.join(".config/solana/id.json"))
                .context("Could not determine home directory")?;
            read_keypair_file(&default_path)
                .map_err(|e| anyhow!("Failed to read payer keypair from {:?}. Use --keypair to specify a path: {}", default_path, e))
        }
    }
}
