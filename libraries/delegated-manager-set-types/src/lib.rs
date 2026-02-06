//! ABI-compatible types for reading `delegated-manager-set` accounts from pure Rust.
//!
//! The manager set contains 7 SEC1 compressed secp256k1 public keys with a 3-byte prefix.
//!
//! ```ignore
//! // 1. Derive the index PDA for Dogecoin (chain_id = 65)
//! let (index_pda, _) = ManagerSetIndex::pda(DOGECOIN_CHAIN_ID);
//!
//! // 2. Deserialize it (validates owner + discriminator)
//! let idx = ManagerSetIndex::deserialize(&index_account_info)?;
//!
//! // 3. Derive the set PDA for that chain + current index
//! let (set_pda, _) = ManagerSet::pda(DOGECOIN_CHAIN_ID, idx.current_index);
//!
//! // 4. Deserialize it
//! let set = ManagerSet::deserialize(&set_account_info)?;
//!
//! // 5. Extract the 7 compressed public keys (skip 3-byte prefix)
//! let compressed_keys = &set.manager_set[3..]; // 231 bytes = 7 * 33
//! ```

use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey};

/// Dogecoin chain ID per Wormhole standard
pub const DOGECOIN_CHAIN_ID: u16 = 65;

/// Manager set prefix bytes (always 01 05 07)
pub const MANAGER_SET_PREFIX: [u8; 3] = [0x01, 0x05, 0x07];

/// Size of SEC1 compressed public key
pub const COMPRESSED_PUBKEY_SIZE: usize = 33;

/// Number of signers in the custodian set
pub const NUM_SIGNERS: usize = 7;

/// Total size of compressed keys section (7 * 33 = 231 bytes)
pub const COMPRESSED_KEYS_SIZE: usize = NUM_SIGNERS * COMPRESSED_PUBKEY_SIZE;

/// Total manager_set size including prefix (3 + 231 = 234 bytes)
pub const MANAGER_SET_DATA_SIZE: usize = MANAGER_SET_PREFIX.len() + COMPRESSED_KEYS_SIZE;

pub mod id {
    solana_program::declare_id!("wdmsTJP6YnsfeQjPuuEzGCrHmZvTmNy8VkxMCK8JkBX");
}
pub use id::ID as PROGRAM_ID;

/// sha256("account:ManagerSetIndex")[..8]
pub const MANAGER_SET_INDEX_DISC: [u8; 8] = [42, 93, 41, 30, 20, 230, 157, 75];
/// sha256("account:ManagerSet")[..8]
pub const MANAGER_SET_DISC: [u8; 8] = [188, 16, 135, 64, 103, 222, 63, 182];

// ---------------------------------------------------------------------------

/// PDA seeds: `["manager_set_index", chain_id(BE)]`
///
/// Layout: `[disc:8][manager_chain_id:2(LE)][current_index:4(LE)]`
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct ManagerSetIndex {
    pub manager_chain_id: u16,
    pub current_index: u32,
}

impl ManagerSetIndex {
    pub const SEED: &'static [u8] = b"manager_set_index";
    pub const SIZE: usize = 8 + 2 + 4;

    pub fn pda(chain_id: u16) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED, &chain_id.to_be_bytes()], &PROGRAM_ID)
    }

    pub fn deserialize(info: &AccountInfo) -> Result<Self, ProgramError> {
        if info.owner != &PROGRAM_ID {
            return Err(ProgramError::IncorrectProgramId);
        }
        let data = info.try_borrow_data()?;
        if data.len() < Self::SIZE || data[..8] != MANAGER_SET_INDEX_DISC {
            return Err(ProgramError::InvalidAccountData);
        }
        Self::try_from_slice(&data[8..]).map_err(|_| ProgramError::InvalidAccountData)
    }
}

// ---------------------------------------------------------------------------

/// PDA seeds: `["manager_set", chain_id(BE), set_index(BE)]`
///
/// Layout: `[disc:8][manager_chain_id:2(LE)][index:4(LE)][len:4(LE)][manager_set:...]`
///
/// The `manager_set` field contains:
/// - 3 bytes prefix: `01 05 07`
/// - 7 SEC1 compressed secp256k1 public keys (33 bytes each):
///   - 1 byte: `02` or `03` (y-parity prefix)
///   - 32 bytes: x-coordinate
/// - Total: 234 bytes
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct ManagerSet {
    pub manager_chain_id: u16,
    pub index: u32,
    pub manager_set: Vec<u8>,
}

impl ManagerSet {
    pub const SEED: &'static [u8] = b"manager_set";

    pub fn size(data_len: usize) -> usize {
        8 + 2 + 4 + 4 + data_len
    }

    pub fn pda(chain_id: u16, set_index: u32) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                Self::SEED,
                &chain_id.to_be_bytes(),
                &set_index.to_be_bytes(),
            ],
            &PROGRAM_ID,
        )
    }

    pub fn deserialize(info: &AccountInfo) -> Result<Self, ProgramError> {
        if info.owner != &PROGRAM_ID {
            return Err(ProgramError::IncorrectProgramId);
        }
        let data = info.try_borrow_data()?;
        if data.len() < 8 || data[..8] != MANAGER_SET_DISC {
            return Err(ProgramError::InvalidAccountData);
        }
        Self::try_from_slice(&data[8..]).map_err(|_| ProgramError::InvalidAccountData)
    }

    /// Extracts the 7 compressed public keys from the manager_set data.
    /// Skips the 3-byte prefix (01 05 07) and returns the 231 bytes of compressed keys.
    pub fn get_compressed_keys(&self) -> Result<&[u8], ProgramError> {
        if self.manager_set.len() != MANAGER_SET_DATA_SIZE {
            return Err(ProgramError::InvalidAccountData);
        }
        if self.manager_set[..3] != MANAGER_SET_PREFIX {
            return Err(ProgramError::InvalidAccountData);
        }
        Ok(&self.manager_set[3..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_set_data_size() {
        // 3 byte prefix + 7 * 33 byte compressed keys = 234 bytes
        assert_eq!(MANAGER_SET_DATA_SIZE, 234);
    }

    #[test]
    fn test_compressed_keys_size() {
        // 7 * 33 = 231 bytes
        assert_eq!(COMPRESSED_KEYS_SIZE, 231);
    }

    #[test]
    fn test_pda_derivation() {
        let (index_pda, _) = ManagerSetIndex::pda(DOGECOIN_CHAIN_ID);
        let (set_pda, _) = ManagerSet::pda(DOGECOIN_CHAIN_ID, 0);

        // Just verify they're valid pubkeys (not equal to each other)
        assert_ne!(index_pda, set_pda);
    }
}
