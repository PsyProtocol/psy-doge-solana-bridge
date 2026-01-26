//! Doge Bridge User Client
//!
//! A Rust client for end-users to interact with the Doge bridge on Solana.
//!
//! # Features
//!
//! - Create associated token accounts for DOGE tokens
//! - Transfer DOGE tokens between Solana accounts
//! - Request withdrawals to Dogecoin addresses
//! - Set close authority to null for token accounts
//!
//! # Example
//!
//! ```ignore
//! use doge_bridge_user_client::UserClient;
//! use solana_sdk::signature::Keypair;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let user_keypair = Keypair::new();
//!     let client = UserClient::new("https://api.mainnet-beta.solana.com")?;
//!
//!     // Create a token account for DOGE
//!     let signature = client.create_token_account(&user_keypair).await?;
//!     println!("Token account created: {}", signature);
//!
//!     Ok(())
//! }
//! ```

mod client;
mod config;
mod errors;
mod instructions;
mod manual_claim_client;

pub use client::UserClient;
pub use config::{UserClientConfig, UserClientConfigBuilder};
pub use errors::{UserClientError, UserClientResult};
pub use manual_claim_client::{
    ManualClaimClient, ManualClaimClientConfig, ManualClaimClientConfigBuilder,
    ParsedManualClaim, DEFAULT_MANUAL_CLAIM_PROGRAM_ID,
};

// Re-export commonly used types
pub use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature},
};
