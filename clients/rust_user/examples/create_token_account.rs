//! Example: Create a DOGE token account for a user.
//!
//! Run with:
//! ```bash
//! cargo run --example create_token_account -- <RPC_URL> <PAYER_KEYPAIR_PATH>
//! ```

use doge_bridge_user_client::UserClient;
use solana_sdk::{signature::read_keypair_file, signer::Signer};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <RPC_URL> <PAYER_KEYPAIR_PATH>", args[0]);
        eprintln!("Example: {} https://api.devnet.solana.com ~/.config/solana/id.json", args[0]);
        std::process::exit(1);
    }

    let rpc_url = &args[1];
    let keypair_path = &args[2];

    // Load the payer keypair
    let payer = read_keypair_file(keypair_path)
        .map_err(|e| format!("Failed to read keypair from {}: {}", keypair_path, e))?;

    println!("Creating UserClient connected to: {}", rpc_url);
    let client = UserClient::new(rpc_url)?;

    println!("Payer public key: {}", payer.pubkey());

    // Get the DOGE mint address
    let doge_mint = client.get_doge_mint().await?;
    println!("DOGE mint address: {}", doge_mint);

    // Check if token account already exists
    if client.token_account_exists(&payer.pubkey()).await? {
        let token_account = client.get_token_account_address(&payer.pubkey()).await?;
        println!("Token account already exists: {}", token_account);

        // Get current balance
        let balance = client.get_balance(&payer.pubkey()).await?;
        println!("Current balance: {} satoshis ({} DOGE)", balance, balance as f64 / 100_000_000.0);
    } else {
        println!("Creating token account for payer...");

        match client.create_token_account(&payer, None).await {
            Ok((signature, token_account)) => {
                println!("Token account created successfully!");
                println!("  Token account: {}", token_account);
                println!("  Transaction signature: {}", signature);
            }
            Err(e) => {
                eprintln!("Failed to create token account: {}", e);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
