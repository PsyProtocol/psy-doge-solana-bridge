use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use clap::Args;
use mpl_token_metadata::{
    instructions::CreateMetadataAccountV3Builder,
    types::DataV2,
    ID as TOKEN_METADATA_PROGRAM_ID,
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use spl_token::state::Mint;

use psy_doge_solana_core::{
    instructions::doge_bridge::{InitializeBridgeInstructionDataDoge, InitializeBridgeParams},
    programs::DOGE_BRIDGE_PROGRAM_ID_STR,
};
use doge_bridge_client::instructions::initialize_bridge;

const BRIDGE_STATE_SEED: &[u8] = b"bridge_state";

/// Default DOGE token metadata
const DOGE_TOKEN_NAME: &str = "Dogecoin";
const DOGE_TOKEN_SYMBOL: &str = "PDOGE";
const DOGE_TOKEN_URI: &str = "https://fs.psydoge.com/metadata/pdoge.json";

/// Output structure saved after initialization
#[derive(serde::Serialize, Debug)]
pub struct InitializationOutput {
    pub bridge_state_pda: String,
    pub bridge_state_pda_hex: String,
    pub doge_mint: String,
    pub doge_mint_hex: String,
    pub doge_mint_metadata_pda: Option<String>,
    pub operator_pubkey: String,
    pub operator_pubkey_hex: String,
    pub fee_spender_pubkey: String,
    pub fee_spender_pubkey_hex: String,
    pub payer_pubkey: String,
    pub payer_pubkey_hex: String,
    pub initialize_tx_signature: String,
    pub create_mint_tx_signature: Option<String>,
    pub create_metadata_tx_signature: Option<String>,
}

#[derive(Args)]
pub struct InitializeFromDogeArgs {
    /// Path to JSON configuration file containing InitializeBridgeInstructionDataDoge
    #[arg(long, short = 'c')]
    config: PathBuf,

    /// Directory for keypair files (operator, fee_spender, payer, doge_mint)
    #[arg(long, default_value = "./bridge-config/keys")]
    keys_dir: PathBuf,

    /// Number of decimals for the DOGE token (default: 8)
    #[arg(long, default_value = "8")]
    decimals: u8,

    /// Output path to save initialization results JSON
    #[arg(long, short = 'o', default_value = "./bridge-config/bridge-output.json")]
    output: PathBuf,

    /// Skip confirmation prompt
    #[arg(long, short = 'y')]
    yes: bool,

    /// Request airdrop from localhost if payer balance is low (for local development)
    #[arg(long)]
    airdrop: bool,
}

pub fn execute(rpc_url: &str, payer_keypair_path: Option<PathBuf>, args: InitializeFromDogeArgs) -> Result<()> {
    let client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

    // Create keys directory if it doesn't exist
    fs::create_dir_all(&args.keys_dir)
        .with_context(|| format!("Failed to create keys directory: {:?}", args.keys_dir))?;

    // Load or create keypairs
    println!("Loading/creating keypairs from {:?}...", args.keys_dir);

    let payer = if let Some(path) = payer_keypair_path {
        load_keypair_from_path(&path)?
    } else {
        load_or_create_keypair(&args.keys_dir, "payer")?
    };
    println!("  Payer:       {} (hex: {})", payer.pubkey(), hex::encode(payer.pubkey().to_bytes()));

    // Request airdrop if needed (for local development)
    // Uses the solana CLI to handle proxy issues correctly
    if args.airdrop {
        let balance = client.get_balance(&payer.pubkey()).unwrap_or(0);
        let min_balance = 10_000_000_000; // 10 SOL in lamports
        if balance < min_balance {
            println!("  Requesting airdrop for payer (current balance: {} SOL)...", balance as f64 / 1_000_000_000.0);
            // Use solana CLI for airdrop to handle proxy correctly
            let output = std::process::Command::new("solana")
                .args(["airdrop", "100", &payer.pubkey().to_string(), "--url", rpc_url])
                .env("no_proxy", "localhost,127.0.0.1")
                .env("NO_PROXY", "localhost,127.0.0.1")
                .output();
            match output {
                Ok(out) => {
                    if out.status.success() {
                        println!("  Airdrop successful");
                    } else {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        println!("  Airdrop failed: {}", stderr.trim());
                    }
                }
                Err(e) => {
                    println!("  Failed to run solana CLI for airdrop: {}", e);
                }
            }
            let new_balance = client.get_balance(&payer.pubkey()).unwrap_or(0);
            println!("  Payer balance: {} SOL", new_balance as f64 / 1_000_000_000.0);
        } else {
            println!("  Payer balance: {} SOL (sufficient)", balance as f64 / 1_000_000_000.0);
        }
    }

    let operator = load_or_create_keypair(&args.keys_dir, "operator")?;
    println!("  Operator:    {} (hex: {})", operator.pubkey(), hex::encode(operator.pubkey().to_bytes()));

    let fee_spender = load_or_create_keypair(&args.keys_dir, "fee_spender")?;
    println!("  Fee Spender: {} (hex: {})", fee_spender.pubkey(), hex::encode(fee_spender.pubkey().to_bytes()));

    // Load and parse config file - uses the serde-serialized InitializeBridgeInstructionDataDoge format
    let config_content = fs::read_to_string(&args.config)
        .with_context(|| format!("Failed to read config file: {:?}", args.config))?;
    let doge_data: InitializeBridgeInstructionDataDoge = serde_json::from_str(&config_content)
        .with_context(|| "Failed to parse config file as InitializeBridgeInstructionDataDoge JSON")?;

    println!("\nParsed configuration from {:?}:", args.config);
    println!("  Tip block height: {}", doge_data.bridge_header.tip_state.block_height);
    println!("  Finalized block height: {}", doge_data.bridge_header.finalized_state.block_height);
    println!("  Start return output index: {}", doge_data.start_return_txo_output.output_index);
    println!("  Start return amount (sats): {}", doge_data.start_return_txo_output.amount_sats);
    println!("  Deposit fee: {}/{} + {} sats flat",
        doge_data.config_params.deposit_fee_rate_numerator,
        doge_data.config_params.deposit_fee_rate_denominator,
        doge_data.config_params.deposit_flat_fee_sats);
    println!("  Withdrawal fee: {}/{} + {} sats flat",
        doge_data.config_params.withdrawal_fee_rate_numerator,
        doge_data.config_params.withdrawal_fee_rate_denominator,
        doge_data.config_params.withdrawal_flat_fee_sats);
    println!("  Custodian wallet config hash: {}", hex::encode(doge_data.custodian_wallet_config_hash));

    // Derive bridge state PDA
    let bridge_program_id = Pubkey::from_str(DOGE_BRIDGE_PROGRAM_ID_STR)?;
    let (bridge_state_pda, _) = Pubkey::find_program_address(&[BRIDGE_STATE_SEED], &bridge_program_id);
    println!("\n  Bridge State PDA: {} (hex: {})", bridge_state_pda, hex::encode(bridge_state_pda.to_bytes()));

    // Check if DOGE mint exists or needs to be created
    // We check both keypair file existence AND on-chain account existence
    // (validator reset wipes on-chain accounts but not keypair files)
    let doge_mint_path = args.keys_dir.join("doge_mint.json");
    let (doge_mint_keypair, keypair_existed) = load_or_create_mint_keypair(&doge_mint_path)?;
    let doge_mint = doge_mint_keypair.pubkey();

    // Check if the mint account actually exists on-chain
    let mint_exists_onchain = client.get_account(&doge_mint).is_ok();
    let mint_needs_creation = !mint_exists_onchain;

    println!("  DOGE Mint:   {} (hex: {})",
        doge_mint,
        hex::encode(doge_mint.to_bytes()),
    );
    if keypair_existed && !mint_exists_onchain {
        println!("    (keypair exists but mint not on-chain - will recreate)");
    } else if mint_exists_onchain {
        println!("    (existing on-chain)");
    } else {
        println!("    (will create)");
    }

    // Confirmation prompt
    if !args.yes {
        println!("\nThis will:");
        if mint_needs_creation {
            println!("  1. Create DOGE mint with bridge PDA as authority");
        }
        println!("  {}. Initialize the bridge program", if !mint_needs_creation { "1" } else { "2" });
        println!("\nThis action cannot be undone.");
        print!("Continue? [y/N] ");
        use std::io::{self, Write};
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    let mut create_mint_signature: Option<String> = None;
    let mut create_metadata_signature: Option<String> = None;
    let mut metadata_pda_str: Option<String> = None;

    // Derive metadata PDA (needed for both creation and output)
    let (metadata_pda, _) = Pubkey::find_program_address(
        &[
            b"metadata",
            TOKEN_METADATA_PROGRAM_ID.as_ref(),
            doge_mint.as_ref(),
        ],
        &TOKEN_METADATA_PROGRAM_ID,
    );

    // Step 1: Create DOGE mint if it doesn't exist on-chain
    // Strategy: Create mint with payer as initial authority, create metadata, then transfer authority to bridge PDA
    if mint_needs_creation {
        println!("\nCreating DOGE mint (with payer as initial authority)...");

        let mint_rent = client
            .get_minimum_balance_for_rent_exemption(Mint::LEN)
            .context("Failed to get rent exemption")?;

        let create_account_ix = system_instruction::create_account(
            &payer.pubkey(),
            &doge_mint,
            mint_rent,
            Mint::LEN as u64,
            &spl_token::id(),
        );

        // Use payer as initial mint authority (will transfer to bridge PDA after metadata creation)
        let init_mint_ix = spl_token::instruction::initialize_mint2(
            &spl_token::id(),
            &doge_mint,
            &payer.pubkey(),
            None, // No freeze authority
            args.decimals,
        )?;

        let recent_blockhash = client
            .get_latest_blockhash()
            .context("Failed to get latest blockhash")?;

        let transaction = Transaction::new_signed_with_payer(
            &[create_account_ix, init_mint_ix],
            Some(&payer.pubkey()),
            &[&payer, &doge_mint_keypair],
            recent_blockhash,
        );

        let signature = client
            .send_and_confirm_transaction(&transaction)
            .context("Failed to create DOGE mint")?;

        println!("  DOGE mint created: {}", signature);
        create_mint_signature = Some(signature.to_string());

        // Step 1b: Create token metadata (payer is mint authority so can sign)
        println!("\nCreating token metadata...");

        let metadata_data = DataV2 {
            name: DOGE_TOKEN_NAME.to_string(),
            symbol: DOGE_TOKEN_SYMBOL.to_string(),
            uri: DOGE_TOKEN_URI.to_string(),
            seller_fee_basis_points: 0,
            creators: None,
            collection: None,
            uses: None,
        };

        let create_metadata_ix = CreateMetadataAccountV3Builder::new()
            .metadata(metadata_pda)
            .mint(doge_mint)
            .mint_authority(payer.pubkey())
            .payer(payer.pubkey())
            .update_authority(payer.pubkey(), true)
            .data(metadata_data)
            .is_mutable(true)
            .instruction();

        let recent_blockhash = client
            .get_latest_blockhash()
            .context("Failed to get latest blockhash for metadata")?;

        let metadata_tx = Transaction::new_signed_with_payer(
            &[create_metadata_ix],
            Some(&payer.pubkey()),
            &[&payer],
            recent_blockhash,
        );

        match client.send_and_confirm_transaction(&metadata_tx) {
            Ok(sig) => {
                println!("  Metadata created: {}", sig);
                println!("  Metadata PDA: {}", metadata_pda);
                println!("  Name: {}", DOGE_TOKEN_NAME);
                println!("  Symbol: {}", DOGE_TOKEN_SYMBOL);
                create_metadata_signature = Some(sig.to_string());
                metadata_pda_str = Some(metadata_pda.to_string());
            }
            Err(e) => {
                println!("  Warning: Failed to create metadata: {}", e);
                println!("  Metadata can be created later if needed.");
            }
        }

        // Step 1c: Transfer mint authority to bridge PDA
        println!("\nTransferring mint authority to bridge PDA...");

        let set_authority_ix = spl_token::instruction::set_authority(
            &spl_token::id(),
            &doge_mint,
            Some(&bridge_state_pda),
            spl_token::instruction::AuthorityType::MintTokens,
            &payer.pubkey(),
            &[],
        )?;

        let recent_blockhash = client
            .get_latest_blockhash()
            .context("Failed to get latest blockhash for authority transfer")?;

        let authority_tx = Transaction::new_signed_with_payer(
            &[set_authority_ix],
            Some(&payer.pubkey()),
            &[&payer],
            recent_blockhash,
        );

        let authority_sig = client
            .send_and_confirm_transaction(&authority_tx)
            .context("Failed to transfer mint authority to bridge PDA")?;

        println!("  Mint authority transferred to: {}", bridge_state_pda);
        println!("  Transaction: {}", authority_sig);
    }

    // Step 2: Initialize bridge
    println!("\nInitializing bridge...");

    let init_params = InitializeBridgeParams {
        bridge_header: doge_data.bridge_header,
        start_return_txo_output: doge_data.start_return_txo_output,
        config_params: doge_data.config_params,
        custodian_wallet_config_hash: doge_data.custodian_wallet_config_hash,
    };

    let init_ix = initialize_bridge(
        payer.pubkey(),
        operator.pubkey(),
        fee_spender.pubkey(),
        doge_mint,
        &init_params,
    );

    let recent_blockhash = client
        .get_latest_blockhash()
        .context("Failed to get latest blockhash")?;

    let transaction = Transaction::new_signed_with_payer(
        &[init_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    let signature = client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to initialize bridge")?;

    println!("  Bridge initialized: {}", signature);

    // Print summary
    println!("\n========== INITIALIZATION COMPLETE ==========");
    println!("Bridge State PDA:  {}", bridge_state_pda);
    println!("  (hex):           {}", hex::encode(bridge_state_pda.to_bytes()));
    println!("DOGE Mint:         {}", doge_mint);
    println!("  (hex):           {}", hex::encode(doge_mint.to_bytes()));
    println!("Operator:          {}", operator.pubkey());
    println!("  (hex):           {}", hex::encode(operator.pubkey().to_bytes()));
    println!("Fee Spender:       {}", fee_spender.pubkey());
    println!("  (hex):           {}", hex::encode(fee_spender.pubkey().to_bytes()));
    println!("Payer:             {}", payer.pubkey());
    println!("  (hex):           {}", hex::encode(payer.pubkey().to_bytes()));
    println!("==============================================");

    // Save output JSON
    let output_data = InitializationOutput {
        bridge_state_pda: bridge_state_pda.to_string(),
        bridge_state_pda_hex: hex::encode(bridge_state_pda.to_bytes()),
        doge_mint: doge_mint.to_string(),
        doge_mint_hex: hex::encode(doge_mint.to_bytes()),
        doge_mint_metadata_pda: metadata_pda_str,
        operator_pubkey: operator.pubkey().to_string(),
        operator_pubkey_hex: hex::encode(operator.pubkey().to_bytes()),
        fee_spender_pubkey: fee_spender.pubkey().to_string(),
        fee_spender_pubkey_hex: hex::encode(fee_spender.pubkey().to_bytes()),
        payer_pubkey: payer.pubkey().to_string(),
        payer_pubkey_hex: hex::encode(payer.pubkey().to_bytes()),
        initialize_tx_signature: signature.to_string(),
        create_mint_tx_signature: create_mint_signature,
        create_metadata_tx_signature: create_metadata_signature,
    };

    // Create parent directories if needed
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {:?}", parent))?;
    }

    let json = serde_json::to_string_pretty(&output_data)?;
    fs::write(&args.output, json)
        .with_context(|| format!("Failed to write output to {:?}", args.output))?;
    println!("\nOutput saved to: {:?}", args.output);

    Ok(())
}

fn load_keypair_from_path(path: &Path) -> Result<Keypair> {
    read_keypair_file(path)
        .map_err(|e| anyhow!("Failed to read keypair from {:?}: {}", path, e))
}

fn load_or_create_keypair(keys_dir: &Path, name: &str) -> Result<Keypair> {
    let path = keys_dir.join(format!("{}.json", name));

    if path.exists() {
        println!("  Loading existing {} keypair from {:?}", name, path);
        read_keypair_file(&path)
            .map_err(|e| anyhow!("Failed to read {} keypair from {:?}: {}", name, path, e))
    } else {
        println!("  Creating new {} keypair at {:?}", name, path);
        let keypair = Keypair::new();
        let keypair_bytes: Vec<u8> = keypair.to_bytes().to_vec();
        let json = serde_json::to_string(&keypair_bytes)?;
        fs::write(&path, json)
            .with_context(|| format!("Failed to write {} keypair to {:?}", name, path))?;
        Ok(keypair)
    }
}

fn load_or_create_mint_keypair(path: &Path) -> Result<(Keypair, bool)> {
    if path.exists() {
        let keypair = read_keypair_file(path)
            .map_err(|e| anyhow!("Failed to read mint keypair from {:?}: {}", path, e))?;
        Ok((keypair, true))
    } else {
        let keypair = Keypair::new();
        let keypair_bytes: Vec<u8> = keypair.to_bytes().to_vec();
        let json = serde_json::to_string(&keypair_bytes)?;
        fs::write(path, json)
            .with_context(|| format!("Failed to write mint keypair to {:?}", path))?;
        Ok((keypair, false))
    }
}
