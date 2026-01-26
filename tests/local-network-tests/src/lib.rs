pub mod local_test_client;
pub mod local_bridge_context;
pub mod local_block_transition_helper;

pub use local_test_client::{LocalTestClient, LocalClientConfig, ProgramIds, TxResult};
pub use local_bridge_context::{LocalBridgeContext, LocalBridgeContextBuilder};
pub use local_block_transition_helper::{LocalBlockTransitionHelper, BTAutoClaimedDeposit};

use std::fs;
use std::path::Path;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};

/// Load a program keypair from the program-keys directory
pub fn load_program_keypair(name: &str) -> anyhow::Result<Keypair> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let keypair_path = Path::new(manifest_dir)
        .join("program-keys")
        .join(format!("{}.json", name));

    let keypair_data = fs::read_to_string(&keypair_path)
        .map_err(|e| anyhow::anyhow!("Failed to read keypair from {:?}: {}", keypair_path, e))?;

    let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_data)
        .map_err(|e| anyhow::anyhow!("Failed to parse keypair JSON: {}", e))?;

    Keypair::from_bytes(&keypair_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to create keypair from bytes: {}", e))
}

/// Get program ID from keypair file
pub fn get_program_id(name: &str) -> anyhow::Result<Pubkey> {
    let keypair = load_program_keypair(name)?;
    Ok(keypair.pubkey())
}

/// Print all program IDs for debugging
pub fn print_program_ids() -> anyhow::Result<()> {
    println!("=== Local Network Test Program IDs ===");
    println!("Doge Bridge: {}", get_program_id("doge-bridge")?);
    println!("Pending Mint: {}", get_program_id("pending-mint")?);
    println!("TXO Buffer: {}", get_program_id("txo-buffer")?);
    println!("Generic Buffer: {}", get_program_id("generic-buffer")?);
    println!("Manual Claim: {}", get_program_id("manual-claim")?);
    Ok(())
}
