//! Example: Monitor noop shim calls and print payload hex
//!
//! This example connects to a Solana RPC and monitors the noop shim program
//! for withdrawal messages, printing the hex-encoded payload (sighash + doge tx bytes).
//!
//! Usage:
//!   cargo run --example noop_shim_monitor -- <RPC_URL> <BRIDGE_STATE_PDA>
//!
//! Example:
//!   cargo run --example noop_shim_monitor -- https://api.devnet.solana.com <your_bridge_pda>

use doge_bridge_client::noop_shim_monitor::{NoopShimMonitor, NoopShimMonitorConfig, NOOP_SHIM_PROGRAM_ID};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing for logs
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <RPC_URL> <BRIDGE_STATE_PDA>", args[0]);
        eprintln!("Example: {} https://api.devnet.solana.com 11111111111111111111111111111111", args[0]);
        std::process::exit(1);
    }

    let rpc_url = &args[1];
    let bridge_state_pda = Pubkey::from_str(&args[2])?;

    println!("Starting noop shim monitor...");
    println!("  RPC URL: {}", rpc_url);
    println!("  Bridge State PDA: {}", bridge_state_pda);
    println!("  Noop Shim Program: {}", NOOP_SHIM_PROGRAM_ID);
    println!();

    let config = NoopShimMonitorConfig::new(rpc_url, bridge_state_pda)
        .poll_interval_ms(1000)
        .batch_size(100);

    let mut monitor = NoopShimMonitor::new(config)?;
    let mut receiver = monitor.subscribe();

    // Start monitoring from the most recent transaction
    let _handle = monitor.start(None).await?;

    println!("Monitoring for noop shim withdrawal messages...");
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
            msg = receiver.recv() => {
                match msg {
                    Some(msg) => {
                        println!("=== Withdrawal Message ===");
                        println!("  Signature: {}", msg.signature);
                        println!("  Slot: {}", msg.slot);
                        if let Some(block_time) = msg.block_time {
                            println!("  Block Time: {}", block_time);
                        }
                        println!("  Nonce: {}", msg.nonce);
                        println!("  Consistency Level: {}", msg.consistency_level);
                        println!("  Emitter: {}", msg.emitter);
                        println!("  Payer: {}", msg.payer);
                        println!();
                        println!("  Sighash (hex): {}", hex::encode(&msg.sighash));
                        println!("  Doge TX Bytes (hex): {}", hex::encode(&msg.doge_tx_bytes));
                        println!("  Doge TX Length: {} bytes", msg.doge_tx_bytes.len());
                        println!();

                        // Combined payload (sighash + doge tx bytes)
                        let mut full_payload = Vec::with_capacity(32 + msg.doge_tx_bytes.len());
                        full_payload.extend_from_slice(&msg.sighash);
                        full_payload.extend_from_slice(&msg.doge_tx_bytes);
                        println!("  Full Payload (hex): {}", hex::encode(&full_payload));
                        println!("=============================\n");
                    }
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
