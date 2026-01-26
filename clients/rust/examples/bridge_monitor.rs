//! Example: Monitor all bridge events
//!
//! This example connects to a Solana RPC and monitors the bridge program
//! for all events: withdrawal requests, processed withdrawals, manual deposits,
//! and block transitions.
//!
//! Usage:
//!   cargo run --example bridge_monitor -- <RPC_URL> <PROGRAM_ID> <BRIDGE_STATE_PDA>
//!
//! Example:
//!   cargo run --example bridge_monitor -- https://api.devnet.solana.com <program_id> <bridge_pda>

use doge_bridge_client::monitor::{BridgeEvent, BridgeMonitor, MonitorConfig};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing for logs
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: {} <RPC_URL> <PROGRAM_ID> <BRIDGE_STATE_PDA>", args[0]);
        eprintln!(
            "Example: {} https://api.devnet.solana.com 11111111111111111111111111111111 22222222222222222222222222222222",
            args[0]
        );
        std::process::exit(1);
    }

    let rpc_url = &args[1];
    let program_id = Pubkey::from_str(&args[2])?;
    let bridge_state_pda = Pubkey::from_str(&args[3])?;

    println!("Starting bridge monitor...");
    println!("  RPC URL: {}", rpc_url);
    println!("  Program ID: {}", program_id);
    println!("  Bridge State PDA: {}", bridge_state_pda);
    println!();

    let config = MonitorConfig::new(rpc_url, program_id, bridge_state_pda)
        .poll_interval_ms(1000)
        .batch_size(100);

    let mut monitor = BridgeMonitor::new(config)?;
    let mut receiver = monitor.subscribe();

    // Start monitoring from the most recent transaction
    let _handle = monitor.start(None).await?;

    println!("Monitoring for bridge events...");
    println!("Press Ctrl+C to stop.\n");

    // Handle Ctrl+C gracefully
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            _ = &mut ctrl_c => {
                println!("\nShutting down...");
                break;
            }
            event = receiver.recv() => {
                match event {
                    Some(event) => print_event(&event),
                    None => {
                        println!("Monitor channel closed");
                        break;
                    }
                }
            }
        }
    }

    drop(_handle);
    Ok(())
}

fn print_event(event: &BridgeEvent) {
    match event {
        BridgeEvent::WithdrawalRequested(e) => {
            println!("=== Withdrawal Requested ===");
            println!("  Signature: {}", e.signature);
            println!("  Slot: {}", e.slot);
            if let Some(block_time) = e.block_time {
                println!("  Block Time: {}", block_time);
            }
            println!("  Amount: {} sats", e.amount_sats);
            println!("  Recipient Address: {}", hex::encode(e.recipient_address));
            println!("  Address Type: {} ({})", e.address_type, if e.address_type == 0 { "P2PKH" } else { "P2SH" });
            println!("  User Pubkey: {}", e.user_pubkey);
            println!("  Withdrawal Index: {}", e.withdrawal_index);
            println!("============================\n");
        }
        BridgeEvent::WithdrawalProcessed(e) => {
            println!("=== Withdrawal Processed ===");
            println!("  Signature: {}", e.signature);
            println!("  Slot: {}", e.slot);
            if let Some(block_time) = e.block_time {
                println!("  Block Time: {}", block_time);
            }
            println!("  New Return Output Sighash: {}", hex::encode(e.new_return_output_sighash));
            println!("  New Return Output Index: {}", e.new_return_output_index);
            println!("  New Return Output Amount: {} sats", e.new_return_output_amount);
            println!("  New Spent TXO Tree Root: {}", hex::encode(e.new_spent_txo_tree_root));
            println!("  New Next Processed Index: {}", e.new_next_processed_withdrawals_index);
            println!("============================\n");
        }
        BridgeEvent::ManualDepositClaimed(e) => {
            println!("=== Manual Deposit Claimed ===");
            println!("  Signature: {}", e.signature);
            println!("  Slot: {}", e.slot);
            if let Some(block_time) = e.block_time {
                println!("  Block Time: {}", block_time);
            }
            println!("  Doge TX Hash: {}", hex::encode(e.tx_hash));
            println!("  Combined TXO Index: {}", e.combined_txo_index);
            println!("  Deposit Amount: {} sats", e.deposit_amount_sats);
            println!("  Depositor Pubkey: {}", hex::encode(e.depositor_pubkey));
            println!("  Claimer Pubkey: {}", e.claimer_pubkey);
            println!("==============================\n");
        }
        BridgeEvent::BlockTransition(e) => {
            println!("=== Block Transition ===");
            println!("  Signature: {}", e.signature);
            println!("  Slot: {}", e.slot);
            if let Some(block_time) = e.block_time {
                println!("  Block Time: {}", block_time);
            }
            println!("  Block Height: {}", e.block_height);
            println!("  Is Reorg: {}", e.is_reorg);
            println!("========================\n");
        }
    }
}
