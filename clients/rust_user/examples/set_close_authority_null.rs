//! Example: Set the close authority of a token account to null.
//!
//! This prevents the token account from being closed, which can be useful
//! for security purposes or to ensure the account persists.
//!
//! Run with:
//! ```bash
//! cargo run --example set_close_authority_null -- <RPC_URL> <OWNER_KEYPAIR_PATH>
//! ```

use doge_bridge_user_client::UserClient;
use solana_sdk::{signature::read_keypair_file, signer::Signer};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <RPC_URL> <OWNER_KEYPAIR_PATH>", args[0]);
        eprintln!(
            "Example: {} https://api.devnet.solana.com ~/.config/solana/id.json",
            args[0]
        );
        std::process::exit(1);
    }

    let rpc_url = &args[1];
    let keypair_path = &args[2];

    // Load the owner keypair
    let owner = read_keypair_file(keypair_path)
        .map_err(|e| format!("Failed to read keypair from {}: {}", keypair_path, e))?;

    println!("Creating UserClient connected to: {}", rpc_url);
    let client = UserClient::new(rpc_url)?;

    println!("Owner public key: {}", owner.pubkey());

    // Check if token account exists
    if !client.token_account_exists(&owner.pubkey()).await? {
        eprintln!("Token account does not exist for this owner.");
        eprintln!("Create one first with: cargo run --example create_token_account");
        std::process::exit(1);
    }

    let token_account = client.get_token_account_address(&owner.pubkey()).await?;
    println!("Token account: {}", token_account);

    println!("Setting close authority to null...");
    println!("Warning: This action is irreversible. The token account cannot be closed after this.");

    match client.set_close_authority_to_null(&owner, None).await {
        Ok(signature) => {
            println!("Close authority set to null successfully!");
            println!("  Transaction signature: {}", signature);
            println!("  The token account can no longer be closed.");
        }
        Err(e) => {
            eprintln!("Failed to set close authority: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
