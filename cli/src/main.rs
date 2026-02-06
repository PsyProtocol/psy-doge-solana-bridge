use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;

use commands::{
    create_dogemint::CreateDogemintArgs,
    create_user::CreateUserArgs,
    custodian_transition::{
        NotifyCustodianUpdateArgs, PauseForTransitionArgs,
        CancelTransitionArgs, TransitionStatusArgs,
    },
    generate_keys::GenerateKeysArgs,
    initialize::InitializeBridgeArgs,
    initialize_from_doge::InitializeFromDogeArgs,
    setup_user_atas::SetupUserAtasArgs,
};

#[derive(Parser)]
#[command(name = "doge-bridge-cli")]
#[command(about = "CLI tool for managing the Doge Bridge on Solana", long_about = None)]
#[command(version)]
struct Cli {
    /// Solana RPC URL
    #[arg(long, default_value = "http://127.0.0.1:8899", global = true)]
    rpc_url: String,

    /// Path to payer keypair file
    #[arg(long, short = 'k', global = true)]
    keypair: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate new keypairs for operator, fee_spender, and payer accounts
    GenerateKeys(GenerateKeysArgs),

    /// Create a new SPL token mint for DOGE
    CreateDogemint(CreateDogemintArgs),

    /// Initialize the bridge program with configuration
    InitializeBridge(InitializeBridgeArgs),

    /// Initialize bridge from Doge data: creates keys if missing, creates mint, initializes bridge
    InitializeFromDogeData(InitializeFromDogeArgs),

    /// Create a new user account with keypair and DOGE token ATA
    CreateUser(CreateUserArgs),

    /// Setup ATAs for existing users and optionally set close authority to null
    SetupUserAtas(SetupUserAtasArgs),

    /// Notify the bridge of a new custodian config from the custodian set manager
    NotifyCustodianUpdate(NotifyCustodianUpdateArgs),

    /// Pause deposits for custodian transition after grace period has elapsed
    PauseForTransition(PauseForTransitionArgs),

    /// Cancel a pending custodian transition (emergency use)
    CancelTransition(CancelTransitionArgs),

    /// Query the current custodian transition status
    TransitionStatus(TransitionStatusArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::GenerateKeys(args) => commands::generate_keys::execute(args),
        Commands::CreateDogemint(args) => commands::create_dogemint::execute(&cli.rpc_url, cli.keypair, args),
        Commands::InitializeBridge(args) => commands::initialize::execute(&cli.rpc_url, cli.keypair, args),
        Commands::InitializeFromDogeData(args) => commands::initialize_from_doge::execute(&cli.rpc_url, cli.keypair, args),
        Commands::CreateUser(args) => commands::create_user::execute(&cli.rpc_url, cli.keypair, args),
        Commands::SetupUserAtas(args) => commands::setup_user_atas::execute(&cli.rpc_url, cli.keypair, args),
        Commands::NotifyCustodianUpdate(args) => commands::custodian_transition::execute_notify_custodian_update(&cli.rpc_url, cli.keypair, args),
        Commands::PauseForTransition(args) => commands::custodian_transition::execute_pause_for_transition(&cli.rpc_url, cli.keypair, args),
        Commands::CancelTransition(args) => commands::custodian_transition::execute_cancel_transition(&cli.rpc_url, cli.keypair, args),
        Commands::TransitionStatus(args) => commands::custodian_transition::execute_transition_status(&cli.rpc_url, args),
    }
}
