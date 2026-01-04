use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Solana Client Error: {0}")]
    SolanaClientError(#[from] solana_client::client_error::ClientError),
    #[error("Solana Program Error: {0}")]
    SolanaProgramError(#[from] solana_sdk::program_error::ProgramError),
    #[error("Signer Error")]
    SignerError,
    #[error("Buffer Error: {0}")]
    BufferError(String),
    #[error("Invalid Input: {0}")]
    InvalidInput(String),
    #[error("Serialization Error: {0}")]
    SerializationError(String),
    #[error("Other Error: {0}")]
    Other(#[from] anyhow::Error),
}