//! Example: Request a withdrawal from Solana to a Dogecoin address.
//!
//! Run with:
//! ```bash
//! cargo run --example withdraw_to_doge -- <RPC_URL> <USER_KEYPAIR_PATH> <DOGE_ADDRESS> <AMOUNT_SATS>
//! ```
//!
//! The Dogecoin address should be a P2PKH address (starts with 'D' on mainnet).

use doge_bridge_user_client::UserClient;
use solana_sdk::{signature::read_keypair_file, signer::Signer};
use std::env;

/// Decode a Dogecoin P2PKH address to get the 20-byte pubkey hash.
///
/// Dogecoin P2PKH addresses use Base58Check encoding with version byte 0x1e (mainnet).
fn decode_doge_address(address: &str) -> Result<[u8; 20], String> {
    // Decode from base58
    let decoded = bs58::decode(address)
        .into_vec()
        .map_err(|e| format!("Invalid base58: {}", e))?;

    if decoded.len() != 25 {
        return Err(format!(
            "Invalid address length: expected 25 bytes, got {}",
            decoded.len()
        ));
    }

    // Verify checksum (last 4 bytes)
    let payload = &decoded[..21];
    let checksum = &decoded[21..];

    use sha2::{Digest, Sha256};
    let hash1 = Sha256::digest(payload);
    let hash2 = Sha256::digest(&hash1);
    let expected_checksum = &hash2[..4];

    if checksum != expected_checksum {
        return Err("Invalid checksum".to_string());
    }

    // Check version byte (0x1e for mainnet P2PKH, 0x71 for testnet)
    let version = decoded[0];
    if version != 0x1e && version != 0x71 {
        return Err(format!(
            "Unsupported address version: 0x{:02x} (expected 0x1e for mainnet or 0x71 for testnet)",
            version
        ));
    }

    // Extract the 20-byte pubkey hash
    let mut pubkey_hash = [0u8; 20];
    pubkey_hash.copy_from_slice(&decoded[1..21]);

    Ok(pubkey_hash)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 5 {
        eprintln!(
            "Usage: {} <RPC_URL> <USER_KEYPAIR_PATH> <DOGE_ADDRESS> <AMOUNT_SATS>",
            args[0]
        );
        eprintln!(
            "Example: {} https://api.devnet.solana.com ~/.config/solana/id.json D8vFz...xyz 1000000",
            args[0]
        );
        std::process::exit(1);
    }

    let rpc_url = &args[1];
    let keypair_path = &args[2];
    let doge_address = &args[3];
    let amount_sats: u64 = args[4]
        .parse()
        .map_err(|_| "Invalid amount: must be a positive integer")?;

    // Load the user keypair
    let user = read_keypair_file(keypair_path)
        .map_err(|e| format!("Failed to read keypair from {}: {}", keypair_path, e))?;

    // Decode the Dogecoin address
    let recipient_hash = decode_doge_address(doge_address)?;
    println!("Dogecoin address: {}", doge_address);
    println!("Pubkey hash (hex): {}", hex::encode(recipient_hash));

    println!("Creating UserClient connected to: {}", rpc_url);
    let client = UserClient::new(rpc_url)?;

    println!("User Solana address: {}", user.pubkey());
    println!(
        "Withdrawal amount: {} satoshis ({} DOGE)",
        amount_sats,
        amount_sats as f64 / 100_000_000.0
    );

    // Check user balance
    let balance = client.get_balance(&user.pubkey()).await?;
    println!(
        "Current balance: {} satoshis ({} DOGE)",
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

    println!("Requesting withdrawal to Dogecoin...");
    println!("Note: The bridge operator will process this withdrawal and send DOGE to your address.");

    // Address type 0 = P2PKH
    match client
        .request_withdrawal(&user, recipient_hash, amount_sats, 0)
        .await
    {
        Ok(signature) => {
            println!("Withdrawal request submitted successfully!");
            println!("  Transaction signature: {}", signature);

            // Show updated balance
            let new_balance = client.get_balance(&user.pubkey()).await?;
            println!(
                "  New balance: {} satoshis ({} DOGE)",
                new_balance,
                new_balance as f64 / 100_000_000.0
            );
            println!(
                "  Tokens burned: {} satoshis",
                balance.saturating_sub(new_balance)
            );
        }
        Err(e) => {
            eprintln!("Failed to request withdrawal: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}
