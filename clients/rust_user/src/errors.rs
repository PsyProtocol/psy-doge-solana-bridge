//! Error types for the user client.

use thiserror::Error;

/// Result type for user client operations.
pub type UserClientResult<T> = Result<T, UserClientError>;

/// Errors that can occur when using the user client.
#[derive(Error, Debug)]
pub enum UserClientError {
    /// RPC client error
    #[error("RPC error: {0}")]
    Rpc(#[from] solana_client::client_error::ClientError),

    /// Invalid input provided
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Account not found
    #[error("Account not found: {address}")]
    AccountNotFound { address: String },

    /// Configuration error
    #[error("Configuration error: {message}")]
    InvalidConfig { message: String },

    /// Transaction failed
    #[error("Transaction failed: {message}")]
    TransactionFailed { message: String },

    /// Token account already exists
    #[error("Token account already exists: {address}")]
    TokenAccountExists { address: String },

    /// Insufficient balance
    #[error("Insufficient balance: required {required}, available {available}")]
    InsufficientBalance { required: u64, available: u64 },
}
