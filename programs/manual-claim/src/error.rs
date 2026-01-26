use solana_program::program_error::ProgramError;
use thiserror::Error;
use psy_bridge_core::error::DogeBridgeError;

#[derive(Error, Debug, Copy, Clone)]
pub enum ManualClaimError {
    #[error("Core Error")]
    CoreError,
    #[error("Serialization Error")]
    SerializationError,
    #[error("Invalid PDA")]
    InvalidPDA,
    #[error("Invalid Recipient ATA")]
    InvalidRecipientATA,
}

impl From<ManualClaimError> for ProgramError {
    fn from(e: ManualClaimError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl From<DogeBridgeError> for ManualClaimError {
    fn from(_: DogeBridgeError) -> Self {
        ManualClaimError::CoreError
    }
}
