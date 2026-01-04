use solana_program::program_error::ProgramError;
use thiserror::Error;
use psy_bridge_core::error::DogeBridgeError;

#[derive(Error, Debug, Copy, Clone)]
pub enum BridgeError {
    #[error("Core Bridge Error")]
    CoreError,
    #[error("Serialization Error")]
    SerializationError,
    #[error("Invalid PDA")]
    InvalidPDA,
    #[error("Invalid Account Input")]
    InvalidAccountInput,
    #[error("Unauthorized Manual Claimer")]
    UnauthorizedManualClaimer,
}

impl From<BridgeError> for ProgramError {
    fn from(e: BridgeError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl From<DogeBridgeError> for BridgeError {
    fn from(_: DogeBridgeError) -> Self {
        BridgeError::CoreError
    }
}
