//! Update the delegated manager set to a new custodian set index.
//!
//! This command transitions to a new custodian set by creating a new ManagerSet
//! at the specified index and updating the ManagerSetIndex to point to it.

use anyhow::{Context, Result};
use clap::Args;
use delegated_manager_set_types::{ManagerSet, ManagerSetIndex, MANAGER_SET_PREFIX};
use serde::Deserialize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    signature::{read_keypair_file, Signer},
    system_program,
    transaction::Transaction,
};
use std::{fs, path::PathBuf};

/// YAML configuration for delegated manager update
#[derive(Debug, Deserialize)]
pub struct DelegatedManagerConfig {
    /// List of 7 custodian wallet public keys (hex-encoded SEC1 compressed format)
    pub custodian_wallet_public_keys: Vec<String>,
}

/// Arguments for the update-delegated-manager command
#[derive(Args, Debug)]
pub struct UpdateDelegatedManagerArgs {
    /// Path to YAML config file containing new custodian public keys
    #[arg(long, short = 'c')]
    pub config: PathBuf,

    /// New custodian set index to transition to
    #[arg(long)]
    pub new_index: u32,

    /// Chain ID (defaults to 65 for Dogecoin)
    #[arg(long, default_value = "65")]
    pub chain_id: u16,
}

fn parse_hex_pubkey(hex: &str) -> Result<[u8; 33]> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    let bytes = hex::decode(hex).context("Invalid hex string for public key")?;
    if bytes.len() != 33 {
        anyhow::bail!(
            "Compressed public key must be 33 bytes, got {}",
            bytes.len()
        );
    }
    let mut arr = [0u8; 33];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

pub fn execute(
    rpc_url: &str,
    keypair_path: Option<PathBuf>,
    args: UpdateDelegatedManagerArgs,
) -> Result<()> {
    // Read and parse YAML config
    let config_contents =
        fs::read_to_string(&args.config).context("Failed to read config file")?;
    let config: DelegatedManagerConfig =
        serde_yaml::from_str(&config_contents).context("Failed to parse YAML config")?;

    // Validate we have exactly 7 public keys
    if config.custodian_wallet_public_keys.len() != 7 {
        anyhow::bail!(
            "Expected exactly 7 custodian public keys, got {}",
            config.custodian_wallet_public_keys.len()
        );
    }

    // Parse all public keys
    let mut compressed_keys: Vec<[u8; 33]> = Vec::with_capacity(7);
    for (i, key_hex) in config.custodian_wallet_public_keys.iter().enumerate() {
        let key = parse_hex_pubkey(key_hex)
            .with_context(|| format!("Failed to parse public key at index {}", i))?;
        compressed_keys.push(key);
    }

    // Build the manager_set data: 3-byte prefix + 231 bytes of compressed keys
    let mut manager_set_data = Vec::with_capacity(234);
    manager_set_data.extend_from_slice(&MANAGER_SET_PREFIX);
    for key in &compressed_keys {
        manager_set_data.extend_from_slice(key);
    }

    let client = RpcClient::new(rpc_url.to_string());

    let payer = keypair_path
        .map(|p| read_keypair_file(&p))
        .transpose()
        .map_err(|e| anyhow::anyhow!("{:?}", e))
        .context("Failed to read payer keypair")?
        .unwrap_or_else(|| {
            read_keypair_file(&shellexpand::tilde("~/.config/solana/id.json").to_string())
                .expect("Failed to read default keypair")
        });

    // Derive PDAs
    let chain_id = args.chain_id;
    let new_index = args.new_index;
    let (manager_set_index_pda, _) = ManagerSetIndex::pda(chain_id);
    let (manager_set_pda, _) = ManagerSet::pda(chain_id, new_index);

    // Get current index for display
    let current_index = match client.get_account_data(&manager_set_index_pda) {
        Ok(data) if data.len() >= 14 => {
            let index_bytes: [u8; 4] = data[10..14].try_into().unwrap();
            Some(u32::from_le_bytes(index_bytes))
        }
        _ => None,
    };

    println!("Updating delegated manager set...");
    println!("  Config file: {:?}", args.config);
    println!("  Chain ID: {} (Dogecoin)", chain_id);
    if let Some(curr) = current_index {
        println!("  Current index: {}", curr);
    }
    println!("  New index: {}", new_index);
    println!("  Manager Set Index PDA: {}", manager_set_index_pda);
    println!("  Manager Set PDA: {}", manager_set_pda);
    println!("\nNew custodian public keys:");
    for (i, key) in compressed_keys.iter().enumerate() {
        println!("  {}: {}", i + 1, hex::encode(key));
    }

    // Build the instruction
    #[derive(borsh::BorshSerialize)]
    struct SetManagerSetArgs {
        chain_id: u16,
        index: u32,
        data: Vec<u8>,
    }

    let args_data = SetManagerSetArgs {
        chain_id,
        index: new_index,
        data: manager_set_data,
    };

    let ix = Instruction {
        program_id: delegated_manager_set_types::PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),
            AccountMeta::new(manager_set_index_pda, false),
            AccountMeta::new(manager_set_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: borsh::to_vec(&args_data)?,
    };

    let recent_blockhash = client.get_latest_blockhash()?;
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer], recent_blockhash);

    let sig = client.send_and_confirm_transaction(&tx)?;
    println!("\nTransaction confirmed: {}", sig);
    println!("Delegated manager set updated to index {}!", new_index);

    Ok(())
}
