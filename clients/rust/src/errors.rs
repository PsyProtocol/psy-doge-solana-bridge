//! Error types for the bridge client.
//!
//! Provides rich error types with retry hints and categorization
//! for better error handling and observability.

use thiserror::Error;

/// Main error type for bridge client operations.
#[derive(Error, Debug)]
pub enum BridgeError {
    // Network Errors
    #[error("RPC error: {0}")]
    Rpc(#[from] solana_client::client_error::ClientError),

    #[error("Rate limited, retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },

    #[error("Connection timeout")]
    ConnectionTimeout,

    // Transaction Errors
    #[error("Simulation failed: {message}")]
    SimulationFailed { message: String, logs: Vec<String> },

    #[error("Confirmation timeout after {timeout_ms}ms")]
    ConfirmationTimeout { timeout_ms: u64 },

    #[error("Transaction rejected: {reason}")]
    TransactionRejected { reason: String },

    #[error("Transaction already processed")]
    AlreadyProcessed,

    // Program Errors
    #[error("Program error: {0}")]
    Program(#[from] solana_sdk::program_error::ProgramError),

    #[error("Invalid bridge state: {message}")]
    InvalidBridgeState { message: String },

    #[error("Pending mints not ready: {reason}")]
    PendingMintsNotReady { reason: String },

    #[error("Block height mismatch: expected {expected}, got {actual}")]
    BlockHeightMismatch { expected: u32, actual: u32 },

    // Buffer Errors
    #[error("Buffer creation failed: {message}")]
    BufferCreationFailed { message: String },

    #[error("Buffer too large: {size} bytes exceeds max {max_size}")]
    BufferTooLarge { size: usize, max_size: usize },

    #[error("Buffer not found: {address}")]
    BufferNotFound { address: String },

    // Configuration Errors
    #[error("Invalid configuration: {message}")]
    InvalidConfig { message: String },

    #[error("Missing required field: {field}")]
    MissingField { field: String },

    // Cryptographic/Proof Errors
    #[error("Invalid ZK proof")]
    InvalidProof,

    #[error("Hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    // Input Validation Errors
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Signer error")]
    SignerError,

    // Account Errors
    #[error("Account not found: {address}")]
    AccountNotFound { address: String },

    #[error("Insufficient balance: need {required}, have {available}")]
    InsufficientBalance { required: u64, available: u64 },

    // Internal Errors
    #[error("{0}")]
    Internal(#[from] anyhow::Error),
}

impl BridgeError {
    /// Check if this error is retryable.
    ///
    /// Retryable errors are typically transient network or rate limiting issues
    /// that may succeed on retry.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            BridgeError::Rpc(_)
                | BridgeError::RateLimited { .. }
                | BridgeError::ConnectionTimeout
                | BridgeError::ConfirmationTimeout { .. }
        )
    }

    /// Get a retry hint in milliseconds, if available.
    ///
    /// Some errors provide a suggested wait time before retrying.
    pub fn retry_hint_ms(&self) -> Option<u64> {
        match self {
            BridgeError::RateLimited { retry_after_ms } => Some(*retry_after_ms),
            BridgeError::ConnectionTimeout => Some(1000),
            BridgeError::ConfirmationTimeout { .. } => Some(2000),
            _ => None,
        }
    }

    /// Categorize the error for metrics and logging.
    pub fn category(&self) -> ErrorCategory {
        match self {
            BridgeError::Rpc(_)
            | BridgeError::RateLimited { .. }
            | BridgeError::ConnectionTimeout => ErrorCategory::Network,

            BridgeError::SimulationFailed { .. }
            | BridgeError::ConfirmationTimeout { .. }
            | BridgeError::TransactionRejected { .. }
            | BridgeError::AlreadyProcessed => ErrorCategory::Transaction,

            BridgeError::Program(_)
            | BridgeError::InvalidBridgeState { .. }
            | BridgeError::PendingMintsNotReady { .. }
            | BridgeError::BlockHeightMismatch { .. } => ErrorCategory::Program,

            BridgeError::BufferCreationFailed { .. }
            | BridgeError::BufferTooLarge { .. }
            | BridgeError::BufferNotFound { .. } => ErrorCategory::Buffer,

            BridgeError::InvalidConfig { .. } | BridgeError::MissingField { .. } => {
                ErrorCategory::Config
            }

            BridgeError::InvalidProof | BridgeError::HashMismatch { .. } => {
                ErrorCategory::Cryptographic
            }

            BridgeError::InvalidInput(_)
            | BridgeError::SerializationError(_)
            | BridgeError::SignerError => ErrorCategory::Validation,

            BridgeError::AccountNotFound { .. } | BridgeError::InsufficientBalance { .. } => {
                ErrorCategory::Account
            }

            BridgeError::Internal(_) => ErrorCategory::Internal,
        }
    }

    /// Create a simulation failed error from logs.
    pub fn simulation_failed(message: impl Into<String>, logs: Vec<String>) -> Self {
        BridgeError::SimulationFailed {
            message: message.into(),
            logs,
        }
    }

    /// Create an invalid bridge state error.
    pub fn invalid_state(message: impl Into<String>) -> Self {
        BridgeError::InvalidBridgeState {
            message: message.into(),
        }
    }

    /// Create a buffer creation failed error.
    pub fn buffer_failed(message: impl Into<String>) -> Self {
        BridgeError::BufferCreationFailed {
            message: message.into(),
        }
    }
}

/// Error category for metrics and logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    /// Network-related errors (RPC, rate limiting, timeouts)
    Network,
    /// Transaction-related errors (simulation, confirmation)
    Transaction,
    /// Program-related errors (on-chain program failures)
    Program,
    /// Buffer-related errors (creation, size limits)
    Buffer,
    /// Configuration errors
    Config,
    /// Cryptographic errors (proofs, hashes)
    Cryptographic,
    /// Input validation errors
    Validation,
    /// Account-related errors (not found, insufficient balance)
    Account,
    /// Internal errors (unexpected failures)
    Internal,
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCategory::Network => write!(f, "network"),
            ErrorCategory::Transaction => write!(f, "transaction"),
            ErrorCategory::Program => write!(f, "program"),
            ErrorCategory::Buffer => write!(f, "buffer"),
            ErrorCategory::Config => write!(f, "config"),
            ErrorCategory::Cryptographic => write!(f, "cryptographic"),
            ErrorCategory::Validation => write!(f, "validation"),
            ErrorCategory::Account => write!(f, "account"),
            ErrorCategory::Internal => write!(f, "internal"),
        }
    }
}

/// Result type alias for bridge operations.
pub type BridgeResult<T> = Result<T, BridgeError>;

// Backward compatibility alias
#[deprecated(since = "0.2.0", note = "Use BridgeError instead")]
pub type ClientError = BridgeError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_errors() {
        assert!(BridgeError::ConnectionTimeout.is_retryable());
        assert!(BridgeError::RateLimited { retry_after_ms: 100 }.is_retryable());
        assert!(!BridgeError::InvalidInput("test".to_string()).is_retryable());
        assert!(!BridgeError::InvalidProof.is_retryable());
    }

    #[test]
    fn test_retry_hints() {
        assert_eq!(
            BridgeError::RateLimited { retry_after_ms: 500 }.retry_hint_ms(),
            Some(500)
        );
        assert_eq!(BridgeError::ConnectionTimeout.retry_hint_ms(), Some(1000));
        assert_eq!(
            BridgeError::InvalidInput("test".to_string()).retry_hint_ms(),
            None
        );
    }

    #[test]
    fn test_error_categories() {
        assert_eq!(
            BridgeError::ConnectionTimeout.category(),
            ErrorCategory::Network
        );
        assert_eq!(
            BridgeError::InvalidProof.category(),
            ErrorCategory::Cryptographic
        );
        assert_eq!(
            BridgeError::InvalidInput("test".to_string()).category(),
            ErrorCategory::Validation
        );
    }
}
