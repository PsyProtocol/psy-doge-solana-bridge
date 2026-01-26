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
use spl_token::instruction as token_instruction;

use crate::commands::create_user::UserAccountInfo;

#[derive(Args)]
pub struct SetupUserAtasArgs {
    /// DOGE token mint address
    #[arg(long, short = 'm')]
    doge_mint: String,

    /// Directory containing user JSON files
    #[arg(long, short = 'd', default_value = "./bridge-config/users")]
    users_dir: PathBuf,

    /// Set close authority to null after creating ATAs (irreversible)
    #[arg(long)]
    set_close_authority_null: bool,

    /// Skip creating the ATA if it already exists (just set close authority if needed)
    #[arg(long)]
    skip_existing: bool,
}

pub fn execute(rpc_url: &str, payer_keypair_path: Option<PathBuf>, args: SetupUserAtasArgs) -> Result<()> {
    let doge_mint: Pubkey = args.doge_mint.parse()
        .with_context(|| format!("Invalid DOGE mint address: {}", args.doge_mint))?;

    let client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

    // Load payer keypair
    let payer = load_payer_keypair(payer_keypair_path)?;
    println!("Payer: {}", payer.pubkey());
    println!("DOGE Mint: {}", doge_mint);
    println!("Users directory: {:?}", args.users_dir);
    println!();

    // Read all user JSON files from the directory
    if !args.users_dir.exists() {
        return Err(anyhow!("Users directory does not exist: {:?}", args.users_dir));
    }

    let entries = fs::read_dir(&args.users_dir)
        .with_context(|| format!("Failed to read users directory: {:?}", args.users_dir))?;

    let mut user_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "json").unwrap_or(false))
        .collect();

    user_files.sort_by_key(|e| e.path());

    if user_files.is_empty() {
        println!("No user JSON files found in {:?}", args.users_dir);
        return Ok(());
    }

    println!("Found {} user files:", user_files.len());
    for entry in &user_files {
        println!("  - {:?}", entry.path().file_name().unwrap_or_default());
    }
    println!();

    let mut success_count = 0;
    let mut error_count = 0;

    for entry in user_files {
        let path = entry.path();
        let filename = path.file_name().unwrap_or_default().to_string_lossy();

        println!("=== Processing {} ===", filename);

        // Load user info from JSON
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                println!("  ERROR: Failed to read file: {}", e);
                error_count += 1;
                continue;
            }
        };

        let user_info: UserAccountInfo = match serde_json::from_str(&content) {
            Ok(info) => info,
            Err(e) => {
                println!("  ERROR: Failed to parse JSON: {}", e);
                error_count += 1;
                continue;
            }
        };

        // Reconstruct the keypair
        let user_keypair = match Keypair::from_bytes(&user_info.private_key) {
            Ok(kp) => kp,
            Err(e) => {
                println!("  ERROR: Failed to reconstruct keypair: {}", e);
                error_count += 1;
                continue;
            }
        };

        let user_pubkey = user_keypair.pubkey();
        println!("  User pubkey: {}", user_pubkey);

        // Derive the ATA address
        let doge_ata = get_associated_token_address(&user_pubkey, &doge_mint);
        println!("  DOGE ATA: {}", doge_ata);

        // Check if ATA already exists
        let ata_exists = client.get_account(&doge_ata).is_ok();

        if ata_exists {
            println!("  ATA already exists on-chain");
            if args.skip_existing && !args.set_close_authority_null {
                println!("  Skipping (--skip-existing)");
                success_count += 1;
                continue;
            }
        } else {
            // Create the ATA
            println!("  Creating ATA...");

            let create_ata_ix = create_associated_token_account(
                &payer.pubkey(),
                &user_pubkey,
                &doge_mint,
                &spl_token::id(),
            );

            let recent_blockhash = match client.get_latest_blockhash() {
                Ok(bh) => bh,
                Err(e) => {
                    println!("  ERROR: Failed to get blockhash: {}", e);
                    error_count += 1;
                    continue;
                }
            };

            let transaction = Transaction::new_signed_with_payer(
                &[create_ata_ix],
                Some(&payer.pubkey()),
                &[&payer],
                recent_blockhash,
            );

            match client.send_and_confirm_transaction(&transaction) {
                Ok(sig) => println!("  ATA created: {}", sig),
                Err(e) => {
                    println!("  ERROR: Failed to create ATA: {}", e);
                    error_count += 1;
                    continue;
                }
            }
        }

        // Set close authority to null if requested
        if args.set_close_authority_null {
            println!("  Setting close authority to null...");

            let set_auth_ix = match token_instruction::set_authority(
                &spl_token::id(),
                &doge_ata,
                None, // Set to null
                token_instruction::AuthorityType::CloseAccount,
                &user_pubkey,
                &[],
            ) {
                Ok(ix) => ix,
                Err(e) => {
                    println!("  ERROR: Failed to create set_authority instruction: {}", e);
                    error_count += 1;
                    continue;
                }
            };

            let recent_blockhash = match client.get_latest_blockhash() {
                Ok(bh) => bh,
                Err(e) => {
                    println!("  ERROR: Failed to get blockhash: {}", e);
                    error_count += 1;
                    continue;
                }
            };

            let transaction = Transaction::new_signed_with_payer(
                &[set_auth_ix],
                Some(&payer.pubkey()),
                &[&payer, &user_keypair],
                recent_blockhash,
            );

            match client.send_and_confirm_transaction(&transaction) {
                Ok(sig) => println!("  Close authority set to null: {}", sig),
                Err(e) => {
                    // This might fail if close authority is already null
                    println!("  Warning: Failed to set close authority (may already be null): {}", e);
                }
            }
        }

        success_count += 1;
        println!();
    }

    println!("========================================");
    println!("Setup complete: {} succeeded, {} failed", success_count, error_count);
    println!("========================================");

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
