use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{anyhow, Context, Result, bail};
use clap::Args;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signer},
    transaction::Transaction,
};

use psy_bridge_core::
    header::{PsyBridgeHeader, PsyBridgeStateCommitment, PsyBridgeTipStateCommitment}
;
use psy_doge_solana_core::{
    instructions::doge_bridge::InitializeBridgeParams,
    program_state::{PsyBridgeConfig, PsyReturnTxOutput},
};
use doge_bridge_client::instructions::initialize_bridge;

/// JSON structure for InitializeBridgeInstructionData configuration file
#[derive(serde::Deserialize, Debug)]
pub struct InitializeBridgeConfigFile {
    /// Operator public key (base58 string)
    pub operator_pubkey: String,
    /// Fee spender public key (base58 string)
    pub fee_spender_pubkey: String,
    /// DOGE token mint address (base58 string)
    pub doge_mint: String,
    /// Bridge header configuration
    pub bridge_header: BridgeHeaderConfig,
    /// Initial return TXO output
    pub start_return_txo_output: ReturnTxOutputConfig,
    /// Fee configuration parameters
    pub config_params: BridgeConfigParams,
    /// Custodian wallet configuration
    pub custodian_wallet_config_hash: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct BridgeHeaderConfig {
    pub tip_state: TipStateConfig,
    pub finalized_state: StateCommitmentConfig,
    /// Bridge state hash (64 char hex string)
    #[serde(default)]
    pub bridge_state_hash: String,
    #[serde(default)]
    pub last_rollback_at_secs: u32,
    #[serde(default)]
    pub paused_until_secs: u32,
    #[serde(default)]
    pub total_finalized_fees_collected_chain_history: u64,
}

#[derive(serde::Deserialize, Debug)]
pub struct TipStateConfig {
    /// Block hash (64 char hex string)
    pub block_hash: String,
    /// Block merkle tree root (64 char hex string)
    pub block_merkle_tree_root: String,
    pub block_time: u32,
    pub block_height: u32,
}

#[derive(serde::Deserialize, Debug)]
pub struct StateCommitmentConfig {
    /// Block hash (64 char hex string)
    pub block_hash: String,
    /// Block merkle tree root (64 char hex string)
    pub block_merkle_tree_root: String,
    /// Pending mints finalized hash (64 char hex string)
    #[serde(default)]
    pub pending_mints_finalized_hash: String,
    /// TXO output list finalized hash (64 char hex string)
    #[serde(default)]
    pub txo_output_list_finalized_hash: String,
    /// Auto claimed TXO tree root (64 char hex string)
    #[serde(default)]
    pub auto_claimed_txo_tree_root: String,
    /// Auto claimed deposits tree root (64 char hex string)
    #[serde(default)]
    pub auto_claimed_deposits_tree_root: String,
    #[serde(default)]
    pub auto_claimed_deposits_next_index: u32,
    pub block_height: u32,
}

#[derive(serde::Deserialize, Debug)]
pub struct ReturnTxOutputConfig {
    /// Transaction sighash (64 char hex string)
    pub sighash: String,
    pub output_index: u64,
    pub amount_sats: u64,
}

#[derive(serde::Deserialize, Debug)]
pub struct BridgeConfigParams {
    #[serde(default)]
    pub deposit_fee_rate_numerator: u64,
    #[serde(default = "default_fee_denominator")]
    pub deposit_fee_rate_denominator: u64,
    #[serde(default)]
    pub withdrawal_fee_rate_numerator: u64,
    #[serde(default = "default_fee_denominator")]
    pub withdrawal_fee_rate_denominator: u64,
    #[serde(default)]
    pub deposit_flat_fee_sats: u64,
    #[serde(default)]
    pub withdrawal_flat_fee_sats: u64,
}

fn default_fee_denominator() -> u64 {
    10000 // Default to basis points (0.01%)
}


#[derive(Args)]
pub struct InitializeBridgeArgs {
    /// Path to JSON configuration file containing InitializeBridgeInstructionData
    #[arg(long, short = 'c')]
    config: PathBuf,

    /// Path to operator keypair file (overrides config file)
    #[arg(long)]
    operator_keypair: Option<PathBuf>,

    /// Path to fee spender keypair file (overrides config file)
    #[arg(long)]
    fee_spender_keypair: Option<PathBuf>,

    /// DOGE mint address (overrides config file)
    #[arg(long)]
    doge_mint: Option<String>,

    /// Skip confirmation prompt
    #[arg(long, short = 'y')]
    yes: bool,
}

pub fn execute(rpc_url: &str, keypair_path: Option<PathBuf>, args: InitializeBridgeArgs) -> Result<()> {
    let client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

    // Load payer keypair
    let payer = load_keypair(keypair_path)?;
    println!("Using payer: {}", payer.pubkey());

    // Load and parse config file
    let config_content = fs::read_to_string(&args.config)
        .with_context(|| format!("Failed to read config file: {:?}", args.config))?;
    let config: InitializeBridgeConfigFile = serde_json::from_str(&config_content)
        .with_context(|| "Failed to parse config file as JSON")?;

    println!("\nParsed configuration:");
    println!("  Config file: {:?}", args.config);

    // Resolve operator pubkey (CLI override or from config)
    let operator_pubkey = if let Some(operator_path) = &args.operator_keypair {
        let keypair = read_keypair_file(operator_path)
            .map_err(|e| anyhow!("Failed to read operator keypair from {:?}: {}", operator_path, e))?;
        keypair.pubkey()
    } else {
        Pubkey::from_str(&config.operator_pubkey)
            .with_context(|| format!("Invalid operator pubkey: {}", config.operator_pubkey))?
    };
    println!("  Operator: {}", operator_pubkey);

    // Resolve fee spender pubkey (CLI override or from config)
    let fee_spender_pubkey = if let Some(fee_spender_path) = &args.fee_spender_keypair {
        let keypair = read_keypair_file(fee_spender_path)
            .map_err(|e| anyhow!("Failed to read fee spender keypair from {:?}: {}", fee_spender_path, e))?;
        keypair.pubkey()
    } else {
        Pubkey::from_str(&config.fee_spender_pubkey)
            .with_context(|| format!("Invalid fee_spender pubkey: {}", config.fee_spender_pubkey))?
    };
    println!("  Fee Spender: {}", fee_spender_pubkey);

    // Resolve DOGE mint (CLI override or from config)
    let doge_mint = if let Some(mint_str) = &args.doge_mint {
        Pubkey::from_str(mint_str)
            .with_context(|| format!("Invalid doge_mint pubkey: {}", mint_str))?
    } else {
        Pubkey::from_str(&config.doge_mint)
            .with_context(|| format!("Invalid doge_mint pubkey: {}", config.doge_mint))?
    };
    println!("  DOGE Mint: {}", doge_mint);

    // Build bridge header
    let bridge_header = build_bridge_header(&config.bridge_header)?;
    println!("  Tip block height: {}", bridge_header.tip_state.block_height);
    println!("  Finalized block height: {}", bridge_header.finalized_state.block_height);

    // Build start return TXO output
    let start_return_txo_output = build_return_txo_output(&config.start_return_txo_output)?;
    println!("  Start return output index: {}", start_return_txo_output.output_index);
    println!("  Start return amount (sats): {}", start_return_txo_output.amount_sats);

    // Build config params
    let config_params = build_config_params(&config.config_params);
    println!("  Deposit fee: {}/{} + {} sats flat",
        config_params.deposit_fee_rate_numerator,
        config_params.deposit_fee_rate_denominator,
        config_params.deposit_flat_fee_sats);
    println!("  Withdrawal fee: {}/{} + {} sats flat",
        config_params.withdrawal_fee_rate_numerator,
        config_params.withdrawal_fee_rate_denominator,
        config_params.withdrawal_flat_fee_sats);

    // Build custodian wallet config
    let custodian_wallet_config_hash: [u8; 32] = hex::decode(&config.custodian_wallet_config_hash)?.try_into().map_err(|_| anyhow::anyhow!("invalid length for custodian_wallet_config_hash"))?;
    println!("  Custodian wallet config hash: {}", hex::encode(custodian_wallet_config_hash));

    // Confirmation prompt
    if !args.yes {
        println!("\nReady to initialize bridge. This action cannot be undone.");
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

    // Build initialization params
    let init_params = InitializeBridgeParams {
        bridge_header,
        start_return_txo_output,
        config_params,
        custodian_wallet_config_hash,
    };

    // Create initialize instruction
    let init_ix = initialize_bridge(
        payer.pubkey(),
        operator_pubkey,
        fee_spender_pubkey,
        doge_mint,
        &init_params,
    );

    // Build and send transaction
    let recent_blockhash = client
        .get_latest_blockhash()
        .context("Failed to get latest blockhash")?;

    let transaction = Transaction::new_signed_with_payer(
        &[init_ix],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    println!("\nSending transaction...");
    let signature = client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to initialize bridge")?;

    println!("\nBridge initialized successfully!");
    println!("  Transaction: {}", signature);

    // Derive and print the bridge state PDA
    let bridge_program_id = Pubkey::from_str(psy_doge_solana_core::programs::DOGE_BRIDGE_PROGRAM_ID_STR)?;
    let (bridge_state_pda, _) = Pubkey::find_program_address(&[b"bridge_state"], &bridge_program_id);
    println!("  Bridge State PDA: {}", bridge_state_pda);

    Ok(())
}

fn load_keypair(keypair_path: Option<PathBuf>) -> Result<Keypair> {
    match keypair_path {
        Some(path) => read_keypair_file(&path)
            .map_err(|e| anyhow!("Failed to read keypair from {:?}: {}", path, e)),
        None => {
            let default_path = dirs::home_dir()
                .map(|h| h.join(".config/solana/id.json"))
                .context("Could not determine home directory")?;
            read_keypair_file(&default_path)
                .map_err(|e| anyhow!("Failed to read keypair from {:?}. Use --keypair to specify a path: {}", default_path, e))
        }
    }
}

fn parse_hex_32(s: &str) -> Result<[u8; 32]> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.is_empty() {
        return Ok([0u8; 32]);
    }
    let bytes = hex::decode(s)
        .with_context(|| format!("Invalid hex string: {}", s))?;
    if bytes.len() != 32 {
        bail!("Expected 32 bytes, got {} bytes for: {}", bytes.len(), s);
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}


fn build_bridge_header(config: &BridgeHeaderConfig) -> Result<PsyBridgeHeader> {
    let tip_state = PsyBridgeTipStateCommitment {
        block_hash: parse_hex_32(&config.tip_state.block_hash)?,
        block_merkle_tree_root: parse_hex_32(&config.tip_state.block_merkle_tree_root)?,
        block_time: config.tip_state.block_time,
        block_height: config.tip_state.block_height,
    };

    let finalized_state = PsyBridgeStateCommitment {
        block_hash: parse_hex_32(&config.finalized_state.block_hash)?,
        block_merkle_tree_root: parse_hex_32(&config.finalized_state.block_merkle_tree_root)?,
        pending_mints_finalized_hash: parse_hex_32(&config.finalized_state.pending_mints_finalized_hash)?,
        txo_output_list_finalized_hash: parse_hex_32(&config.finalized_state.txo_output_list_finalized_hash)?,
        auto_claimed_txo_tree_root: parse_hex_32(&config.finalized_state.auto_claimed_txo_tree_root)?,
        auto_claimed_deposits_tree_root: parse_hex_32(&config.finalized_state.auto_claimed_deposits_tree_root)?,
        auto_claimed_deposits_next_index: config.finalized_state.auto_claimed_deposits_next_index,
        block_height: config.finalized_state.block_height,
    };

    Ok(PsyBridgeHeader {
        tip_state,
        finalized_state,
        bridge_state_hash: parse_hex_32(&config.bridge_state_hash)?,
        last_rollback_at_secs: config.last_rollback_at_secs,
        paused_until_secs: config.paused_until_secs,
        total_finalized_fees_collected_chain_history: config.total_finalized_fees_collected_chain_history,
    })
}

fn build_return_txo_output(config: &ReturnTxOutputConfig) -> Result<PsyReturnTxOutput> {
    Ok(PsyReturnTxOutput {
        sighash: parse_hex_32(&config.sighash)?,
        output_index: config.output_index,
        amount_sats: config.amount_sats,
    })
}

fn build_config_params(config: &BridgeConfigParams) -> PsyBridgeConfig {
    PsyBridgeConfig {
        deposit_fee_rate_numerator: config.deposit_fee_rate_numerator,
        deposit_fee_rate_denominator: config.deposit_fee_rate_denominator,
        withdrawal_fee_rate_numerator: config.withdrawal_fee_rate_numerator,
        withdrawal_fee_rate_denominator: config.withdrawal_fee_rate_denominator,
        deposit_flat_fee_sats: config.deposit_flat_fee_sats,
        withdrawal_flat_fee_sats: config.withdrawal_flat_fee_sats,
    }
}
