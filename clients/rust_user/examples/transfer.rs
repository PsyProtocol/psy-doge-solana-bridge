//! Example: Transfer DOGE tokens to another Solana address.
//!
//! Run with:
//! ```bash
//! cargo run --example transfer -- <RPC_URL> <SENDER_KEYPAIR_PATH> <RECIPIENT_PUBKEY> <AMOUNT_SATS>
//! ```

use doge_bridge_user_client::UserClient;
use solana_sdk::{pubkey::Pubkey, signature::read_keypair_file, signer::Signer};
use std::{env, str::FromStr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 5 {
        eprintln!(
            "Usage: {} <RPC_URL> <SENDER_KEYPAIR_PATH> <RECIPIENT_PUBKEY> <AMOUNT_SATS>",
            args[0]
        );
        eprintln!(
            "Example: {} https://api.devnet.solana.com ~/.config/solana/id.json 9xQe...abc 1000000",
            args[0]
        );
        std::process::exit(1);
    }

    let rpc_url = &args[1];
    let keypair_path = &args[2];
    let recipient_str = &args[3];
    let amount_sats: u64 = args[4]
        .parse()
        .map_err(|_| "Invalid amount: must be a positive integer")?;

    // Load the sender keypair
    let sender = read_keypair_file(keypair_path)
        .map_err(|e| format!("Failed to read keypair from {}: {}", keypair_path, e))?;

    // Parse recipient public key
    let recipient = Pubkey::from_str(recipient_str)
        .map_err(|e| format!("Invalid recipient public key: {}", e))?;

    println!("Creating UserClient connected to: {}", rpc_url);
    let client = UserClient::new(rpc_url)?;

    println!("Sender: {}", sender.pubkey());
    println!("Recipient: {}", recipient);
    println!(
        "Amount: {} satoshis ({} DOGE)",
        amount_sats,
        amount_sats as f64 / 100_000_000.0
    );

    // Check sender balance
    let balance = client.get_balance(&sender.pubkey()).await?;
    println!(
        "Sender balance: {} satoshis ({} DOGE)",
        balance,
        balance as f64 / 100_000_000.0
    );

    if balance < amount_sats {
        eprintln!(
            "Insufficient balance: need {} but only have {}",
            amount_sats, balance
        );
        std::process::exit(1);
    }

    println!("Transferring tokens...");

    match client.transfer(&sender, &recipient, amount_sats).await {
        Ok(signature) => {
            println!("Transfer successful!");
            println!("  Transaction signature: {}", signature);

            // Show updated balances
            let new_sender_balance = client.get_balance(&sender.pubkey()).await?;
            println!(
                "  New sender balance: {} satoshis ({} DOGE)",
                new_sender_balance,
                new_sender_balance as f64 / 100_000_000.0
            );

            if client.token_account_exists(&recipient).await? {
                let recipient_balance = client.get_balance(&recipient).await?;
                println!(
                    "  Recipient balance: {} satoshis ({} DOGE)",
                    recipient_balance,
                    recipient_balance as f64 / 100_000_000.0
                );
            }
        }
        Err(e) => {
            eprintln!("Failed to transfer: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
