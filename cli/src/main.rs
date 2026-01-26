use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;

use commands::{
    create_dogemint::CreateDogemintArgs,
    create_user::CreateUserArgs,
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
    }
}
