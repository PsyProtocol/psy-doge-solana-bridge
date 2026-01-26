use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;
use solana_sdk::signature::{Keypair, Signer};

#[derive(Args)]
pub struct GenerateKeysArgs {
    /// Output directory for keypair files
    #[arg(long, short = 'o', default_value = "./bridge-config/keys")]
    output_dir: PathBuf,

    /// Generate only operator keypair
    #[arg(long)]
    operator_only: bool,

    /// Generate only fee_spender keypair
    #[arg(long)]
    fee_spender_only: bool,

    /// Generate only payer keypair
    #[arg(long)]
    payer_only: bool,
}

pub fn execute(args: GenerateKeysArgs) -> Result<()> {
    // Create output directory if it doesn't exist
    fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("Failed to create output directory: {:?}", args.output_dir))?;

    let generate_all = !args.operator_only && !args.fee_spender_only && !args.payer_only;

    if generate_all || args.operator_only {
        generate_and_save_keypair(&args.output_dir, "operator")?;
    }

    if generate_all || args.fee_spender_only {
        generate_and_save_keypair(&args.output_dir, "fee_spender")?;
    }

    if generate_all || args.payer_only {
        generate_and_save_keypair(&args.output_dir, "payer")?;
    }

    println!("\nKeypairs generated successfully in {:?}", args.output_dir);
    println!("\nIMPORTANT: Keep these keypair files secure and backed up!");
    println!("To fund accounts on devnet/testnet, use: solana airdrop 2 <PUBKEY> --url <RPC_URL>");

    Ok(())
}

fn generate_and_save_keypair(output_dir: &Path, name: &str) -> Result<()> {
    let keypair = Keypair::new();
    let pubkey = keypair.pubkey();
    let keypair_path = output_dir.join(format!("{}.json", name));

    // Save keypair as JSON array of bytes (Solana CLI compatible format)
    let keypair_bytes: Vec<u8> = keypair.to_bytes().to_vec();
    let json = serde_json::to_string(&keypair_bytes)?;
    fs::write(&keypair_path, json)
        .with_context(|| format!("Failed to write keypair to {:?}", keypair_path))?;

    println!("Generated {} keypair:", name);
    println!("  Pubkey (base58): {}", pubkey);
    println!("  Pubkey (hex):    {}", hex::encode(pubkey.to_bytes()));
    println!("  Saved to: {:?}", keypair_path);

    Ok(())
}
