use anyhow::{Context, Result};
use clap::Args;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{read_keypair_file, Signer},
};
use std::path::PathBuf;
use std::str::FromStr;

use doge_bridge_client::instructions;
use doge_bridge_client::constants::DOGE_BRIDGE_PROGRAM_ID;
use psy_doge_solana_core::constants::{
    CUSTODIAN_TRANSITION_GRACE_PERIOD_SECONDS,
    DEPOSITS_PAUSED_MODE_ACTIVE, DEPOSITS_PAUSED_MODE_PAUSED,
};

/// Arguments for the notify-custodian-update command
#[derive(Args, Debug)]
pub struct NotifyCustodianUpdateArgs {
    /// Pubkey of the custodian set manager account
    #[arg(long)]
    pub custodian_account: String,

    /// Expected new custodian config hash (hex string)
    #[arg(long)]
    pub config_hash: String,

    /// Path to operator keypair
    #[arg(long)]
    pub operator_keypair: PathBuf,
}

/// Arguments for the pause-for-transition command
#[derive(Args, Debug)]
pub struct PauseForTransitionArgs {
    /// Path to operator keypair
    #[arg(long)]
    pub operator_keypair: PathBuf,
}

/// Arguments for the cancel-transition command
#[derive(Args, Debug)]
pub struct CancelTransitionArgs {
    /// Path to operator keypair
    #[arg(long)]
    pub operator_keypair: PathBuf,
}

/// Arguments for the transition-status command
#[derive(Args, Debug)]
pub struct TransitionStatusArgs {}

fn parse_hex_hash(hex: &str) -> Result<[u8; 32]> {
    let hex = hex.strip_prefix("0x").unwrap_or(hex);
    let bytes = hex::decode(hex).context("Invalid hex string")?;
    if bytes.len() != 32 {
        anyhow::bail!("Hash must be 32 bytes, got {}", bytes.len());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

pub fn execute_notify_custodian_update(
    rpc_url: &str,
    keypair_path: Option<PathBuf>,
    args: NotifyCustodianUpdateArgs,
) -> anyhow::Result<()> {
    let client = RpcClient::new(rpc_url.to_string());

    let payer = keypair_path
        .map(|p| read_keypair_file(&p))
        .transpose()
        .map_err(|e| anyhow::anyhow!("{:?}",e))
        .context("Failed to read payer keypair")?
        .unwrap_or_else(|| {
            read_keypair_file(&shellexpand::tilde("~/.config/solana/id.json").to_string())
                .expect("Failed to read default keypair")
        });

    let operator = read_keypair_file(&args.operator_keypair).map_err(|e| anyhow::anyhow!("{:?}",e))
        .context("Failed to read operator keypair")?;

    let custodian_account = Pubkey::from_str(&args.custodian_account)
        .context("Invalid custodian account pubkey")?;

    let config_hash = parse_hex_hash(&args.config_hash)?;

    println!("Notifying custodian config update...");
    println!("  Custodian account: {}", custodian_account);
    println!("  Config hash: 0x{}", hex::encode(config_hash));

    let ix = instructions::notify_custodian_config_update(
        DOGE_BRIDGE_PROGRAM_ID,
        operator.pubkey(),
        custodian_account,
        config_hash,
    );

    let recent_blockhash = client.get_latest_blockhash()?;
    let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer, &operator],
        recent_blockhash,
    );

    let sig = client.send_and_confirm_transaction(&tx)?;
    println!("Transaction confirmed: {}", sig);
    println!("Grace period started. Deposits can be paused after {} seconds.", CUSTODIAN_TRANSITION_GRACE_PERIOD_SECONDS);

    Ok(())
}

pub fn execute_pause_for_transition(
    rpc_url: &str,
    keypair_path: Option<PathBuf>,
    args: PauseForTransitionArgs,
) -> Result<()> {
    let client = RpcClient::new(rpc_url.to_string());

    let payer = keypair_path
        .map(|p| read_keypair_file(&p))
        .transpose()
        .map_err(|e| anyhow::anyhow!("{:?}",e))
        .context("Failed to read payer keypair")?
        .unwrap_or_else(|| {
            read_keypair_file(&shellexpand::tilde("~/.config/solana/id.json").to_string())
                .expect("Failed to read default keypair")
        });

    let operator = read_keypair_file(&args.operator_keypair)
        .map_err(|e| anyhow::anyhow!("{:?}",e))
        .context("Failed to read operator keypair")?;

    println!("Pausing deposits for custodian transition...");

    let ix = instructions::pause_deposits_for_transition(
        DOGE_BRIDGE_PROGRAM_ID,
        operator.pubkey(),
    );

    let recent_blockhash = client.get_latest_blockhash()?;
    let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer, &operator],
        recent_blockhash,
    );

    let sig = client.send_and_confirm_transaction(&tx)?;
    println!("Transaction confirmed: {}", sig);
    println!("Deposits are now paused. UTXOs can be consolidated for transition.");

    Ok(())
}

pub fn execute_cancel_transition(
    rpc_url: &str,
    keypair_path: Option<PathBuf>,
    args: CancelTransitionArgs,
) -> Result<()> {
    let client = RpcClient::new(rpc_url.to_string());

    let payer = keypair_path
        .map(|p| read_keypair_file(&p))
        .transpose()
        .map_err(|e| anyhow::anyhow!("{:?}",e))
        .context("Failed to read payer keypair")?
        .unwrap_or_else(|| {
            read_keypair_file(&shellexpand::tilde("~/.config/solana/id.json").to_string())
                .expect("Failed to read default keypair")
        });

    let operator = read_keypair_file(&args.operator_keypair)
        .map_err(|e| anyhow::anyhow!("{:?}",e))
        .context("Failed to read operator keypair")?;

    println!("Cancelling custodian transition...");

    let ix = instructions::cancel_custodian_transition(
        DOGE_BRIDGE_PROGRAM_ID,
        operator.pubkey(),
    );

    let recent_blockhash = client.get_latest_blockhash()?;
    let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[&payer, &operator],
        recent_blockhash,
    );

    let sig = client.send_and_confirm_transaction(&tx)?;
    println!("Transaction confirmed: {}", sig);
    println!("Custodian transition cancelled. Deposits are active again.");

    Ok(())
}

pub fn execute_transition_status(
    rpc_url: &str,
    _args: TransitionStatusArgs,
) -> Result<()> {
    let client = RpcClient::new(rpc_url.to_string());

    let (bridge_state_pda, _) = Pubkey::find_program_address(&[b"bridge_state"], &DOGE_BRIDGE_PROGRAM_ID);

    println!("Fetching custodian transition status...");
    println!("  Bridge state: {}", bridge_state_pda);

    let account_data = client.get_account_data(&bridge_state_pda)
        .context("Failed to fetch bridge state account")?;

    // Parse the BridgeState from account data
    // The structure is: doge_mint (32 bytes) + PsyBridgeProgramState
    if account_data.len() < 32 {
        anyhow::bail!("Account data too small");
    }

    let core_state: &psy_doge_solana_core::program_state::PsyBridgeProgramState =
        bytemuck::from_bytes(&account_data[32..32 + std::mem::size_of::<psy_doge_solana_core::program_state::PsyBridgeProgramState>()]);

    let deposits_paused_mode = core_state.deposits_paused_mode;
    let last_detected = core_state.last_detected_custodian_transition_seconds;
    let incoming_hash = core_state.incoming_transition_custodian_config_hash;
    let current_hash = core_state.custodian_wallet_config_hash;

    println!("\nCustodian Transition Status:");
    println!("  Current custodian config hash: 0x{}", hex::encode(current_hash));
    println!("  Incoming custodian config hash: 0x{}", hex::encode(incoming_hash));
    println!("  Last detected transition (unix): {}", last_detected);
    println!("  Deposits paused mode: {}", match deposits_paused_mode {
        DEPOSITS_PAUSED_MODE_ACTIVE => "Active (deposits allowed)",
        DEPOSITS_PAUSED_MODE_PAUSED => "Paused (consolidation in progress)",
        _ => "Unknown",
    });

    if last_detected > 0 {
        let grace_period_ends = last_detected + CUSTODIAN_TRANSITION_GRACE_PERIOD_SECONDS;
        println!("  Grace period ends at (unix): {}", grace_period_ends);

        if deposits_paused_mode == DEPOSITS_PAUSED_MODE_ACTIVE {
            println!("\nStatus: PENDING - waiting for grace period to elapse");
        } else {
            println!("\nStatus: CONSOLIDATING - UTXOs being consolidated to new custodian");

            let auto_claimed = core_state.bridge_header.finalized_state.auto_claimed_deposits_next_index as u64;
            let manual_deposits = core_state.manual_deposits_tree.next_index as u64;
            let total_target = auto_claimed + manual_deposits;
            let spent = core_state.total_spent_deposit_utxo_count;

            println!("  Consolidation target: {} (auto: {}, manual: {})", total_target, auto_claimed, manual_deposits);
            println!("  Currently spent: {}", spent);
            println!("  Progress: {:.1}%", (spent as f64 / total_target as f64) * 100.0);
        }
    } else {
        println!("\nStatus: NONE - no transition in progress");
    }

    Ok(())
}
