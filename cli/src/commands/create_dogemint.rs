use std::path::PathBuf;

use anyhow::{anyhow, Context};
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

use psy_doge_solana_core::programs::DOGE_BRIDGE_PROGRAM_ID_STR;

/// Default DOGE token metadata
const DOGE_TOKEN_NAME: &str = "Dogecoin";
const DOGE_TOKEN_SYMBOL: &str = "PDOGE";
const DOGE_TOKEN_URI: &str = "https://fs.psydoge.com/metadata/pdoge.json";

const BRIDGE_STATE_SEED: &[u8] = b"bridge_state";

#[derive(Args)]
pub struct CreateDogemintArgs {
    /// Path to mint authority keypair (optional, will use payer if not specified)
    #[arg(long)]
    mint_authority: Option<PathBuf>,

    /// Number of decimals for the DOGE token (default: 8 to match Dogecoin)
    #[arg(long, default_value = "8")]
    decimals: u8,

    /// Output path to save the mint keypair
    #[arg(long, short = 'o')]
    output: Option<PathBuf>,

    /// Use the bridge state PDA as mint authority (for production)
    #[arg(long)]
    bridge_pda_authority: bool,

    /// Token name for metadata (default: "Dogecoin")
    #[arg(long, default_value = DOGE_TOKEN_NAME)]
    token_name: String,

    /// Token symbol for metadata (default: "PDOGE")
    #[arg(long, default_value = DOGE_TOKEN_SYMBOL)]
    token_symbol: String,

    /// Token metadata URI (default: standard Dogecoin metadata JSON)
    #[arg(long, default_value = DOGE_TOKEN_URI)]
    token_uri: String,

    /// Skip metadata creation
    #[arg(long)]
    skip_metadata: bool,
}

pub fn execute(rpc_url: &str, keypair_path: Option<PathBuf>, args: CreateDogemintArgs) -> anyhow::Result<()> {
    let client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

    // Load payer keypair
    let payer = load_keypair(keypair_path)?;
    println!("Using payer: {}", payer.pubkey());

    // Generate new mint keypair
    let mint_keypair = Keypair::new();
    let mint_pubkey = mint_keypair.pubkey();
    println!("Creating DOGE mint: {}", mint_pubkey);

    // Determine final mint authority and initial authority for setup
    let (initial_mint_authority, final_mint_authority) = if args.bridge_pda_authority {
        let bridge_program_id = Pubkey::try_from(DOGE_BRIDGE_PROGRAM_ID_STR)
            .context("Invalid bridge program ID")?;
        let (bridge_state_pda, _) = Pubkey::find_program_address(&[BRIDGE_STATE_SEED], &bridge_program_id);
        println!("Final mint authority will be bridge state PDA: {}", bridge_state_pda);
        println!("Using payer as initial authority for setup...");
        (payer.pubkey(), Some(bridge_state_pda))
    } else if let Some(authority_path) = &args.mint_authority {
        let authority = read_keypair_file(authority_path)
            .map_err(|e| anyhow!("Failed to read mint authority keypair from {:?}: {}", authority_path, e))?;
        println!("Using mint authority: {}", authority.pubkey());
        (authority.pubkey(), None)
    } else {
        println!("Using payer as mint authority: {}", payer.pubkey());
        (payer.pubkey(), None)
    };

    // Calculate rent for mint account
    let mint_rent = client
        .get_minimum_balance_for_rent_exemption(Mint::LEN)
        .context("Failed to get rent exemption")?;

    // Create mint account
    let create_account_ix = system_instruction::create_account(
        &payer.pubkey(),
        &mint_pubkey,
        mint_rent,
        Mint::LEN as u64,
        &spl_token::id(),
    );

    // Initialize mint with initial authority (payer when using bridge PDA)
    let init_mint_ix = spl_token::instruction::initialize_mint2(
        &spl_token::id(),
        &mint_pubkey,
        &initial_mint_authority,
        None, // No freeze authority
        args.decimals,
    )?;

    // Build and send mint creation transaction
    let recent_blockhash = client
        .get_latest_blockhash()
        .context("Failed to get latest blockhash")?;

    let transaction = Transaction::new_signed_with_payer(
        &[create_account_ix, init_mint_ix],
        Some(&payer.pubkey()),
        &[&payer, &mint_keypair],
        recent_blockhash,
    );

    let signature = client
        .send_and_confirm_transaction(&transaction)
        .context("Failed to create DOGE mint")?;

    println!("\nDOGE mint created successfully!");
    println!("  Mint address: {}", mint_pubkey);
    println!("  Initial mint authority: {}", initial_mint_authority);
    println!("  Decimals: {}", args.decimals);
    println!("  Transaction: {}", signature);

    // Create metadata if not skipped
    if !args.skip_metadata {
        println!("\nCreating token metadata...");

        // Derive metadata PDA
        let (metadata_pda, _) = Pubkey::find_program_address(
            &[
                b"metadata",
                TOKEN_METADATA_PROGRAM_ID.as_ref(),
                mint_pubkey.as_ref(),
            ],
            &TOKEN_METADATA_PROGRAM_ID,
        );

        // Load the mint authority keypair if specified, otherwise use payer
        // When using bridge PDA, payer is the initial authority so we use that
        let mint_authority_signer = if args.bridge_pda_authority {
            // Payer is the initial mint authority when using bridge PDA
            Keypair::from_bytes(&payer.to_bytes())
                .map_err(|e| anyhow!("Failed to clone payer keypair: {}", e))?
        } else if let Some(authority_path) = &args.mint_authority {
            read_keypair_file(authority_path)
                .map_err(|e| anyhow!("Failed to read mint authority keypair: {}", e))?
        } else {
            // Payer is the mint authority, we need to clone its bytes
            Keypair::from_bytes(&payer.to_bytes())
                .map_err(|e| anyhow!("Failed to clone payer keypair: {}", e))?
        };

        let metadata_data = DataV2 {
            name: args.token_name.clone(),
            symbol: args.token_symbol.clone(),
            uri: args.token_uri.clone(),
            seller_fee_basis_points: 0,
            creators: None,
            collection: None,
            uses: None,
        };

        let create_metadata_ix = CreateMetadataAccountV3Builder::new()
            .metadata(metadata_pda)
            .mint(mint_pubkey)
            .mint_authority(mint_authority_signer.pubkey())
            .payer(payer.pubkey())
            .update_authority(mint_authority_signer.pubkey(), true)
            .data(metadata_data)
            .is_mutable(true)
            .instruction();

        let recent_blockhash = client
            .get_latest_blockhash()
            .context("Failed to get latest blockhash for metadata")?;

        // Always include mint_authority_signer - even if it has the same pubkey as payer,
        // the Transaction requires the exact keypair instances that will sign
        let metadata_tx = Transaction::new_signed_with_payer(
            &[create_metadata_ix],
            Some(&payer.pubkey()),
            &[&payer, &mint_authority_signer],
            recent_blockhash,
        );

        let metadata_sig = client
            .send_and_confirm_transaction(&metadata_tx)
            .context("Failed to create token metadata")?;

        println!("  Metadata PDA: {}", metadata_pda);
        println!("  Name: {}", args.token_name);
        println!("  Symbol: {}", args.token_symbol);
        println!("  URI: {}", args.token_uri);
        println!("  Transaction: {}", metadata_sig);
    }

    // Transfer mint authority to bridge PDA if requested
    if let Some(new_authority) = final_mint_authority {
        println!("\nTransferring mint authority to bridge PDA...");

        let set_authority_ix = spl_token::instruction::set_authority(
            &spl_token::id(),
            &mint_pubkey,
            Some(&new_authority),
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
            .context("Failed to transfer mint authority")?;

        println!("  New mint authority: {}", new_authority);
        println!("  Transaction: {}", authority_sig);
    }

    // Save mint keypair if output path specified
    if let Some(output_path) = args.output {
        let keypair_bytes: Vec<u8> = mint_keypair.to_bytes().to_vec();
        let json = serde_json::to_string(&keypair_bytes)?;
        std::fs::write(&output_path, json)
            .with_context(|| format!("Failed to write mint keypair to {:?}", output_path))?;
        println!("  Keypair saved to: {:?}", output_path);
    }

    Ok(())
}

fn load_keypair(keypair_path: Option<PathBuf>) -> anyhow::Result<Keypair> {
    match keypair_path {
        Some(path) => read_keypair_file(&path)
            .map_err(|e| anyhow!("Failed to read keypair from {:?}: {}", path, e)),
        None => {
            // Try to load from default Solana CLI path
            let default_path = dirs::home_dir()
                .map(|h| h.join(".config/solana/id.json"))
                .context("Could not determine home directory")?;
            read_keypair_file(&default_path)
                .map_err(|e| anyhow!("Failed to read keypair from {:?}. Use --keypair to specify a path: {}", default_path, e))
        }
    }
}
