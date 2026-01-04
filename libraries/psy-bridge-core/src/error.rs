
//! Error types


#[cfg(feature = "serialize_borsh")]
use borsh::{BorshSerialize, BorshDeserialize};
#[cfg(feature = "serialize_serde")]
use serde::{Serialize, Deserialize};

use num_derive::FromPrimitive;
use thiserror::Error;

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serialize_borsh", derive(BorshSerialize, BorshDeserialize))]
/// Errors that may be returned by the oracle program
#[derive(Clone, Debug, Eq, Error, PartialEq, Copy, FromPrimitive)]
pub enum DogeBridgeError {

    /// 0 - Error deserializing an account
    #[error("Error deserializing an account")]
    DeserializationError = 0,
    /// 1 - Error serializing an account
    #[error("Error serializing an account")]
    SerializationError = 1,
    /// 2 - Invalid program owner
    #[error("Invalid program owner. This likely mean the provided account does not exist")]
    InvalidProgramOwner = 2,
    /// 3 - Invalid PDA derivation
    #[error("Invalid PDA derivation")]
    InvalidPda = 3,
    /// 4 - Expected empty account
    #[error("Expected empty account")]
    ExpectedEmptyAccount = 4,
    /// 5 - Expected non empty account
    #[error("Expected non empty account")]
    ExpectedNonEmptyAccount = 5,
    /// 6 - Expected signer account
    #[error("Expected signer account")]
    ExpectedSignerAccount = 6,
    /// 7 - Expected writable account
    #[error("Expected writable account")]
    ExpectedWritableAccount = 7,
    /// 8 - Account mismatch
    #[error("Account mismatch")]
    AccountMismatch = 8,
    /// 9 - Invalid account key
    #[error("Invalid account key")]
    InvalidAccountKey = 9,
    /// 10 - Numerical overflow
    #[error("Numerical overflow")]
    NumericalOverflow = 10,


    /// Generic catch all error
    #[error("Unknown Error")]
    UnknownError = 600,

    /// start doge bridge demo stuff
    #[error("AuxPow version bits mismatch")]
    AuxPowVersionBitsMismatch = 601,
    #[error("AuxPow chain id mismatch")]
    AuxPowChainIdMismatch = 602,
    #[error("Difficulty bits mismatch")]
    DifficutlyBitsMismatch = 603,
    #[error("Standard proof of work check failed")]
    StandardPoWCheckFailed = 604,
    #[error("AuxPow parent block proof of work check failed")]
    AuxPowParentBlockPoWCheckFailed = 605,
    #[error("coinbase_branch.side_mask != 0, AuxPow is not a generate")]
    AuxPowCoinBaseBranchSideMaskNonZero = 606,
    #[error("Aux POW chain merkle branch too long")]
    AuxPowChainMerkleBranchTooLong = 607,
    #[error("Aux POW parent has our chain ID")]
    AuxPowParentHasOurChainId = 608,
    #[error("Aux POW merkle root incorrect")]
    IncorrectAuxPowMerkleRoot = 609,

    #[error("Aux POW coinbase has no inputs")]
    AuxPowCoinbaseNoInputs = 610,
    #[error("Aux POW missing chain merkle root in parent coinbase")]
    AuxPowCoinbaseMissingChainMerkleRoot = 611,
    #[error("MERGED_MINING_HEADER found twice in coinbase transaction input script")]
    MergedMiningHeaderFoundTwiceInCoinbase = 612,
    #[error("MERGED_MINING_HEADER not found at the beginning of the coinbase transaction input script")]
    MergedMiningHeaderNotFoundAtCoinbaseScriptStart = 613,
    #[error("chain merkle root starts too late in the coinbase transaction input script")]
    AuxPowChainMerkleRootTooLateInCoinbaseInputScript = 614,
    #[error("coinbase transaction input script is too short")]
    AuxPowCoinbaseTransactionInputScriptTooShort = 615,
    #[error("n_size in coinbase script does not correspond to the number of leaves of the merkle tree implictly defined by the blockchain branch hashes length")]
    AuxPowCoinbaseScriptInvalidNSize = 616,
    #[error("the sidemask provided in blockchain branch does not match the one computed from the coinbase transaction script")]
    AuxPowCoinbaseScriptInvalidSideMask = 617,



    /// start doge bridge runner stuff
    #[error("Attempted to fetch a Block at a height that is not stored in the cache (it is either too old or has not been processed yet)")]
    BlockNotInCache = 701,
    #[error("Attempted to modify an already finalized/confirmed block")]
    AttemptedToModifiyFinalizedBlock = 702,
    #[error("Insufficient block provided for rollback in the block data tracker")]
    InsufficientBlocksProvidedForRollback = 703,
    #[error("Attempted to insert a Block that already exists in the cache")]
    InsertBlockAlreadyInCache = 704,
    #[error("Attempted to append a block with a height that is not equal to the current tip + 1")]
    InsertBlockNotAtTip = 705,
    #[error("Parent block hash in block header does not match the current tip")]
    InvalidParentBlockHash = 706,
    #[error("AuxPow missing in aux pow block")]
    AuxPowMissing = 707,
    #[error("AuxPow found in non-aux pow block")]
    AuxPowNotExpected = 708,
    #[error("Block tip sync mismatch error")]
    BlockTipSyncMismatch = 709,
    #[error("The root of the Block tree after rollback doesn't match the known correct root")]
    RollbackBlockTreeRootMismatch = 710,
    #[error("The index of the block tree failed to rollback correctly")]
    RollbackBlockTreeIndexMismatch = 711,


    // start fixed append tree errors
    #[error("Cannot revert to index greater than or equal to current index")]
    RevertIndexTooHigh = 724,
    #[error("Not enough changed left siblings")]
    NotEnoughChangedLeftSiblings = 725,
    #[error("Revert index is not a prefix of current index")]
    RevertIndexNotPrefix = 726,
    #[error("Too many changed left siblings provided")]
    TooManyChangedLeftSiblings = 727,



    #[error("Invalid bridge ZKP")]
    BridgeZKPError = 750,

    #[error("Invalid bridge ZKP provided as input")]
    InvalidBridgeInputZKP = 751,

    #[error("Invalid verifier key for bridge ZKP")]
    InvalidVerifierKeyForBridgeZKP = 752,

    #[error("Invalid public inputs for bridge ZKP")]
    InvalidPublicInputsForBridgeZKP = 753,



    #[error("There are no fees to send to the fee collector")]
    NoFeesToSendToFeeCollector = 800,

    #[error("You cannot rollback to a state before the last finalized block")]
    AttemptedRollbackOfFinalizedBlock = 801,

    #[error("The bit-list buffer provided is too small for the size specified in the bit vector header")]
    BitListBufferTooSmall = 802,
    #[error("The hash of the bit-list or the provided new bit-list hash stack does not match the expected hash")]
    BitListHashMismatch = 803,
    #[error("The user attempted to pop from an empty bit-list hash stack")]
    BitListEmpty = 804,

    #[error("The provided public key and amount or updated pending mint hash stack hash do not match the expected value")]
    AutoProcessMintHashMismatch = 805,
    #[error("The caller attempted to perform a bridge state transition without clearing the pending auto processed mint hash stack")]
    AutoProcessMintHashNotEmpty = 806,


    // bridge program state errors
    #[error("The deposit being processed has already been recorded")]
    DepositAlreadyProcessed = 900,
    #[error("Auto claimed deposit tree root not found or is not recent enough")]
    AutoClaimedDepositTreeRootNotRecentEnough = 901,
    #[error("Block merkle tree root not found or is not recent enough")]
    BlockMerkleTreeRootNotRecentEnough = 902,
    #[error("The request could not be processed due to insufficient fees")]
    InsufficientBridgeFees = 903,

    #[error("processed withdrawals tree root mismatch")]
    ProcessedWithdrawalsTreeRootMismatch = 904,

    #[error("Mints from finalized blocks are still pending processing")]
    PendingFinalizedBlockMintsNotEmpty = 905,
    #[error("No pending mints to process")]
    NoPendingMintsToProcess = 906,
    #[error("Pending mints group index out of bounds")]
    PendingMintsGroupIndexOutOfBounds = 907,

    #[error("Pending mints group already processed")]
    PendingMintsGroupAlreadyProcessed = 908,

    #[error("Invalid pending finalized block mints group count, must be less than or equal to 8")]
    PendingFinalizedBlockMintsInvalidGroupCount = 909,

    #[error("Invalid pending finalized block mints data")]
    PendingFinalizedBlockMintsInvalidAutoClaimMintsData = 910,

    #[error("Program state not ready for new block update")]
    ProgramStateNotReadyForBlockUpdate = 911,

    #[error("Invalid ZK proof size")]
    InvalidZKProofSize = 912,
    #[error("Invalid ZK verifier key size")]
    InvalidZKVerifierKeySize = 913,

    #[error("Invalid block height for single block transition")]
    InvalidBlockHeightForSingleBlockTransition = 914,
    #[error("Auto claimed deposits next index less than previous")]
    InvalidAutoClaimedDepositsNextIndex = 915,

    #[error("Mint buffer PDA is not for the correct buffer builder program")]
    InvalidMintBufferPdaProgram = 916,

    #[error("Mint buffer header group count/deposits count does not match the values in the incoming finalized bridge header")]
    InvalidMintBufferHeaderGroupCountOrDepositsCount = 917,

    #[error("Invalid mint buffer size passed to block update")]
    InvalidAutoClaimMintBufferDataAccountSize = 918,

    #[error("Mint buffer locking permission is not set to the bridge program")]
    InvalidMintBufferLockingPermission = 919,

    #[error("Invalid pending mint groups count in mint buffer header")]
    InvalidMintBufferPendingMintGroupsCount = 920,
    #[error("Invalid pending mints count in mint buffer header")]
    InvalidMintBufferPendingMintsCount = 921,
    #[error("Too many new auto claimed deposits in block update")]
    TooManyNewAutoClaimedDeposits = 922,

    #[error("Invalid pending mints buffer hash")]
    InvalidPendingMintsBufferHash = 923,

    #[error("Invalid auto claim txo buffer size")]
    InvalidAutoClaimTxoBufferDataAccountSize = 924,

    #[error("Invalid auto claim txo buffer pending mints count")]
    InvalidAutoClaimTxoBufferPendingMintsCount = 925,

    #[error("Invalid auto claim txo buffer hash")]
    InvalidAutoClaimTxoBufferHash = 926,

    #[error("Invalid block height for reorg transition")]
    InvalidBlockHeightForReorgTransition = 927,
    #[error("Invalid extra finalized blocks length for reorg transition")]
    InvalidExtraFinalizedBlocksLengthForReorg = 928,
    #[error("Invalid pending mints count for transition")]
    InvalidPendingMintsCountForTransition = 929,

    #[error("no pending mints to auto process")]
    NoPendingMintsToAutoProcess = 930,
    #[error("previous pending mints not completed")]
    RemainingPendingMintsInPreviousState = 931,

    #[error("invalid withdrawal amount")]
    InvalidWithdrawalAmount = 932,
    #[error("insufficient bridge balance for withdrawal")]
    InsufficientBridgeBalanceForWithdrawal = 933,

    #[error("no operator fees to withdraw")]
    NoOperatorFeesToWithdraw = 934,

    #[error("Invalid PDA for manual deposit manager")]
    InvalidPDAForManualDepositManager = 935,


    /// Generic catch all error
    #[error("Error in cpi lock call to mint buffer")]
    CpiLockCallError = 936,

    #[error("Error in cpi lock call to mint buffer")]
    CpiUnlockCallError = 937,

    #[error("Error in cpi send signature request call to doge sig requester program")]
    CpiSendSignatureRequestCallError = 938,
    #[error("Error in cpi create generic buffer call to generic buffer program")]
    CpiCreateGenericBufferCallError = 939,
    #[error("Error in cpi close generic buffer call to generic buffer program")]
    CpiCloseGenericBufferCallError = 940,
    #[error("Error in cpi create pending mint buffer call to pending mint buffer program")]
    CpiCreatePendingMintBufferCallError = 941,
    #[error("Error in cpi create txo buffer call to txo buffer program")]
    CpiCreateTxoBufferCallError = 942,
    #[error("Error in cpi close txo buffer call to txo buffer program")]
    CpiCloseTxoBufferCallError = 943,
    #[error("Error in cpi token mint to call to token program")]
    CpiTokenMintToCallError = 944,
    #[error("Error in cpi burn call to token program")]
    CpiTokenBurnCallError = 945,
    #[error("Error in cpi manual deposit call to manual claim program")]
    CpiManualDepositCallError = 946,

    #[error("Attempted to unlock pending mint buffer when not allowed")]
    AttemptedUnlockPendingMintBuffer = 947,
    #[error("Failed to unlock pending mint buffer when necessary")]
    FailedUnlockPendingMintBuffer = 948,

    #[error("Invalid mint buffer PDA")]
    InvalidMintBufferPDA = 949,
    #[error("Invalid txo buffer PDA")]
    InvalidTxoBufferPDA = 950,

    #[error("Cannot unlock pending mint buffer after auto advancing pending mint state")]
    CannotUnlockAfterAutoAdvance = 951,
}
#[cfg(feature = "solprogram")]
impl solana_program_error::ToStr for DogeBridgeError {
    fn to_str<E>(&self) -> &'static str
    where
        E: 'static + solana_program_error::ToStr + TryFrom<u32>,
    {
        match self {
            DogeBridgeError::DeserializationError => "Error deserializing an account",
            DogeBridgeError::SerializationError => "Error serializing an account",
            DogeBridgeError::InvalidProgramOwner => "Invalid program owner. This likely mean the provided account does not exist",
            DogeBridgeError::InvalidPda => "Invalid PDA derivation",
            DogeBridgeError::ExpectedEmptyAccount => "Expected empty account",
            DogeBridgeError::ExpectedNonEmptyAccount => "Expected non empty account",
            DogeBridgeError::ExpectedSignerAccount => "Expected signer account",
            DogeBridgeError::ExpectedWritableAccount => "Expected writable account",
            DogeBridgeError::AccountMismatch => "Account mismatch",
            DogeBridgeError::InvalidAccountKey => "Invalid account key",
            DogeBridgeError::NumericalOverflow => "Numerical overflow",
            DogeBridgeError::UnknownError => "Unknown Error",
            
            // Doge bridge demo
            DogeBridgeError::AuxPowVersionBitsMismatch => "AuxPow version bits mismatch",
            DogeBridgeError::AuxPowChainIdMismatch => "AuxPow chain id mismatch",
            DogeBridgeError::DifficutlyBitsMismatch => "Difficulty bits mismatch",
            DogeBridgeError::StandardPoWCheckFailed => "Standard proof of work check failed",
            DogeBridgeError::AuxPowParentBlockPoWCheckFailed => "AuxPow parent block proof of work check failed",
            DogeBridgeError::AuxPowCoinBaseBranchSideMaskNonZero => "coinbase_branch.side_mask != 0, AuxPow is not a generate",
            DogeBridgeError::AuxPowChainMerkleBranchTooLong => "Aux POW chain merkle branch too long",
            DogeBridgeError::AuxPowParentHasOurChainId => "Aux POW parent has our chain ID",
            DogeBridgeError::IncorrectAuxPowMerkleRoot => "Incorrect Aux POW merkle root",
            DogeBridgeError::AuxPowCoinbaseNoInputs => "Aux POW coinbase has no inputs",
            DogeBridgeError::AuxPowCoinbaseMissingChainMerkleRoot => "Aux POW missing chain merkle root in parent coinbase",
            DogeBridgeError::MergedMiningHeaderFoundTwiceInCoinbase => "MERGED_MINING_HEADER found twice in coinbase transaction input script",
            DogeBridgeError::MergedMiningHeaderNotFoundAtCoinbaseScriptStart => "MERGED_MINING_HEADER not found at the beginning of the coinbase transaction input script",
            DogeBridgeError::AuxPowChainMerkleRootTooLateInCoinbaseInputScript => "chain merkle root starts too late in the coinbase transaction input script",
            DogeBridgeError::AuxPowCoinbaseTransactionInputScriptTooShort => "coinbase transaction input script is too short",
            DogeBridgeError::AuxPowCoinbaseScriptInvalidNSize => "n_size in coinbase script does not correspond to the number of leaves of the merkle tree implictly defined by the blockchain branch hashes length",
            DogeBridgeError::AuxPowCoinbaseScriptInvalidSideMask => "the sidemask provided in blockchain branch does not match the one computed from the coinbase transaction script",

            // Doge bridge runner
            DogeBridgeError::BlockNotInCache => "Attempted to fetch a Block at a height that is not stored in the cache (it is either too old or has not been processed yet)",
            DogeBridgeError::AttemptedToModifiyFinalizedBlock => "Attempted to modify an already finalized/confirmed block",
            DogeBridgeError::InsufficientBlocksProvidedForRollback => "Insufficient block provided for rollback in the block data tracker",
            DogeBridgeError::InsertBlockAlreadyInCache => "Attempted to insert a Block that already exists in the cache",
            DogeBridgeError::InsertBlockNotAtTip => "Attempted to append a block with a height that is not equal to the current tip + 1",
            DogeBridgeError::InvalidParentBlockHash => "Parent block hash in block header does not match the current tip",
            DogeBridgeError::AuxPowMissing => "AuxPow missing in aux pow block",
            DogeBridgeError::AuxPowNotExpected => "AuxPow found in non-aux pow block",
            DogeBridgeError::BlockTipSyncMismatch => "Block tip sync mismatch error",
            DogeBridgeError::RollbackBlockTreeRootMismatch => "The root of the Block tree after rollback doesn't match the known correct root",
            DogeBridgeError::RollbackBlockTreeIndexMismatch => "The index of the block tree failed to rollback correctly",

            // Fixed append tree
            DogeBridgeError::RevertIndexTooHigh => "Cannot revert to index greater than or equal to current index",
            DogeBridgeError::NotEnoughChangedLeftSiblings => "Not enough changed left siblings",
            DogeBridgeError::RevertIndexNotPrefix => "Revert index is not a prefix of current index",
            DogeBridgeError::TooManyChangedLeftSiblings => "Too many changed left siblings provided",

            // ZKP
            DogeBridgeError::BridgeZKPError => "Invalid bridge ZKP",
            DogeBridgeError::InvalidBridgeInputZKP => "Invalid bridge ZKP provided as input",
            DogeBridgeError::InvalidVerifierKeyForBridgeZKP => "Invalid verifier key for bridge ZKP",
            DogeBridgeError::InvalidPublicInputsForBridgeZKP => "Invalid public inputs for bridge ZKP",

            // State management
            DogeBridgeError::NoFeesToSendToFeeCollector => "There are no fees to send to the fee collector",
            DogeBridgeError::AttemptedRollbackOfFinalizedBlock => "You cannot rollback to a state before the last finalized block",
            DogeBridgeError::BitListBufferTooSmall => "The bit-list buffer provided is too small for the size specified in the bit vector header",
            DogeBridgeError::BitListHashMismatch => "The hash of the bit-list or the provided new bit-list hash stack does not match the expected hash",
            DogeBridgeError::BitListEmpty => "The user attempted to pop from an empty bit-list hash stack",
            DogeBridgeError::AutoProcessMintHashMismatch => "The provided public key and amount or updated pending mint hash stack hash do not match the expected value",
            DogeBridgeError::AutoProcessMintHashNotEmpty => "The caller attempted to perform a bridge state transition without clearing the pending auto processed mint hash stack",

            // Bridge program state
            DogeBridgeError::DepositAlreadyProcessed => "The deposit being processed has already been recorded",
            DogeBridgeError::AutoClaimedDepositTreeRootNotRecentEnough => "Auto claimed deposit tree root not found or is not recent enough",
            DogeBridgeError::BlockMerkleTreeRootNotRecentEnough => "Block merkle tree root not found or is not recent enough",
            DogeBridgeError::InsufficientBridgeFees => "The request could not be processed due to insufficient fees",
            DogeBridgeError::ProcessedWithdrawalsTreeRootMismatch => "processed withdrawals tree root mismatch",
            DogeBridgeError::PendingFinalizedBlockMintsNotEmpty => "Mints from finalized blocks are still pending processing",
            DogeBridgeError::NoPendingMintsToProcess => "No pending mints to process",
            DogeBridgeError::PendingMintsGroupIndexOutOfBounds => "Pending mints group index out of bounds",
            DogeBridgeError::PendingMintsGroupAlreadyProcessed => "Pending mints group already processed",
            DogeBridgeError::PendingFinalizedBlockMintsInvalidGroupCount => "Invalid pending finalized block mints group count, must be less than or equal to 8",
            DogeBridgeError::PendingFinalizedBlockMintsInvalidAutoClaimMintsData => "Invalid pending finalized block mints data",
            DogeBridgeError::ProgramStateNotReadyForBlockUpdate => "Program state not ready for new block update",
            DogeBridgeError::InvalidZKProofSize => "Invalid ZK proof size",
            DogeBridgeError::InvalidZKVerifierKeySize => "Invalid ZK verifier key size",
            DogeBridgeError::InvalidBlockHeightForSingleBlockTransition => "Invalid block height for single block transition",
            DogeBridgeError::InvalidAutoClaimedDepositsNextIndex => "Invalid auto claimed deposits next index less than previous",
            DogeBridgeError::InvalidMintBufferPdaProgram => "Mint buffer PDA is not for the correct buffer builder program",
            DogeBridgeError::InvalidMintBufferHeaderGroupCountOrDepositsCount => "Mint buffer header group count/deposits count does not match the values in the incoming finalized bridge header",
            DogeBridgeError::InvalidAutoClaimMintBufferDataAccountSize => "Invalid mint buffer size passed to block update",
            DogeBridgeError::InvalidMintBufferLockingPermission => "Mint buffer locking permission is not set to the bridge program",
            DogeBridgeError::InvalidMintBufferPendingMintGroupsCount => "Invalid pending mint groups count in mint buffer header",
            DogeBridgeError::InvalidMintBufferPendingMintsCount => "Invalid pending mints count in mint buffer header",
            DogeBridgeError::TooManyNewAutoClaimedDeposits => "Too many new auto claimed deposits in block update",
            DogeBridgeError::InvalidPendingMintsBufferHash => "Invalid pending mints buffer hash",
            DogeBridgeError::InvalidAutoClaimTxoBufferDataAccountSize => "Invalid auto claim txo buffer size",
            DogeBridgeError::InvalidAutoClaimTxoBufferPendingMintsCount => "Invalid auto claim txo buffer pending mints count",
            DogeBridgeError::InvalidAutoClaimTxoBufferHash => "Invalid auto claim txo buffer hash",
            DogeBridgeError::InvalidBlockHeightForReorgTransition => "Invalid block height for reorg transition",
            DogeBridgeError::InvalidExtraFinalizedBlocksLengthForReorg => "Invalid extra finalized blocks length for reorg transition",
            DogeBridgeError::InvalidPendingMintsCountForTransition => "Invalid pending mints count for transition",
            DogeBridgeError::NoPendingMintsToAutoProcess => "no pending mints to auto process",
            DogeBridgeError::RemainingPendingMintsInPreviousState => "previous pending mints not completed",
            DogeBridgeError::InvalidWithdrawalAmount => "invalid withdrawal amount",
            DogeBridgeError::InsufficientBridgeBalanceForWithdrawal => "insufficient bridge balance for withdrawal",
            DogeBridgeError::NoOperatorFeesToWithdraw => "no operator fees to withdraw",
            DogeBridgeError::InvalidPDAForManualDepositManager => "Invalid PDA for manual deposit manager",

            // CPI Errors
            DogeBridgeError::CpiLockCallError => "Error in cpi lock call to mint buffer",
            DogeBridgeError::CpiUnlockCallError => "Error in cpi lock call to mint buffer",
            DogeBridgeError::CpiSendSignatureRequestCallError => "Error in cpi send signature request call to doge sig requester program",
            DogeBridgeError::CpiCreateGenericBufferCallError => "Error in cpi create generic buffer call to generic buffer program",
            DogeBridgeError::CpiCloseGenericBufferCallError => "Error in cpi close generic buffer call to generic buffer program",
            DogeBridgeError::CpiCreatePendingMintBufferCallError => "Error in cpi create pending mint buffer call to pending mint buffer program",
            DogeBridgeError::CpiCreateTxoBufferCallError => "Error in cpi create txo buffer call to txo buffer program",
            DogeBridgeError::CpiCloseTxoBufferCallError => "Error in cpi close txo buffer call to txo buffer program",
            DogeBridgeError::CpiTokenMintToCallError => "Error in cpi token mint to call to token program",
            DogeBridgeError::CpiTokenBurnCallError => "Error in cpi burn call to token program",
            DogeBridgeError::CpiManualDepositCallError => "Error in cpi manual deposit call to manual claim program",

            // Final state checks
            DogeBridgeError::AttemptedUnlockPendingMintBuffer => "Attempted to unlock pending mint buffer when not allowed",
            DogeBridgeError::FailedUnlockPendingMintBuffer => "Failed to unlock pending mint buffer when necessary",
            DogeBridgeError::InvalidMintBufferPDA => "Invalid mint buffer PDA",
            DogeBridgeError::InvalidTxoBufferPDA => "Invalid txo buffer PDA",
            DogeBridgeError::CannotUnlockAfterAutoAdvance => "Cannot unlock pending mint buffer after auto advancing pending mint state",
        }
    }
}

/* 
#[cfg(feature = "solprogram")]
impl solana_program::program_error::PrintProgramError for DogeBridgeError {
    fn print<E>(&self) {
        solana_program::msg!(&self.to_string());
    }
}
    */
#[cfg(feature = "solprogram")]
impl From<DogeBridgeError> for solana_program::program_error::ProgramError {
    fn from(e: DogeBridgeError) -> Self {
        solana_program::program_error::ProgramError::Custom(e as u32)
    }
}
/* 
#[cfg(feature = "solprogram")]
impl<T> solana_program::decode_error::DecodeError<T> for DogeBridgeError {
    fn type_of() -> &'static str {
        "Doge Bridge Error"
    }
}

*/
#[cfg(not(feature = "solprogram"))]
impl From<DogeBridgeError> for solana_program_error::ProgramError {
    fn from(e: DogeBridgeError) -> Self {
        solana_program_error::ProgramError::Custom(e as u32)
    }
}




#[macro_export]
macro_rules! doge_bail {
    ($err:expr $(,)?) => {
        return Err($err);
    };
}


pub type QDogeResult<T> = Result<T, DogeBridgeError>;
