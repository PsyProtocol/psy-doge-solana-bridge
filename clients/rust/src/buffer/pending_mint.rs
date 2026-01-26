//! Pending mint buffer building utilities.
//!
//! Handles creation and population of pending mint buffer accounts
//! for batch token minting operations.

use psy_doge_solana_core::data_accounts::pending_mint::{
    PendingMint, PM_DA_PENDING_MINT_SIZE, PM_MAX_PENDING_MINTS_PER_GROUP,
};
use solana_sdk::pubkey::Pubkey;

/// Header size for pending mint buffer.
pub const PENDING_MINT_BUFFER_HEADER_SIZE: usize = 72;

/// Builder for pending mint buffer data.
pub struct PendingMintBufferBuilder {
    mints: Vec<PendingMint>,
}

impl PendingMintBufferBuilder {
    /// Create a new builder with the given mints.
    pub fn new(mints: Vec<PendingMint>) -> Self {
        Self { mints }
    }

    /// Get the total number of mints.
    pub fn total_mints(&self) -> usize {
        self.mints.len()
    }

    /// Calculate the number of groups needed.
    pub fn num_groups(&self) -> usize {
        if self.mints.is_empty() {
            0
        } else {
            (self.mints.len() + PM_MAX_PENDING_MINTS_PER_GROUP - 1) / PM_MAX_PENDING_MINTS_PER_GROUP
        }
    }

    /// Calculate the total buffer size needed.
    pub fn buffer_size(&self) -> usize {
        let groups = self.num_groups();
        PENDING_MINT_BUFFER_HEADER_SIZE
            + (groups * 32) // Group hashes
            + (self.mints.len() * PM_DA_PENDING_MINT_SIZE)
    }

    /// Get mints for a specific group.
    pub fn get_group(&self, group_idx: usize) -> &[PendingMint] {
        let start = group_idx * PM_MAX_PENDING_MINTS_PER_GROUP;
        let end = std::cmp::min(start + PM_MAX_PENDING_MINTS_PER_GROUP, self.mints.len());
        &self.mints[start..end]
    }

    /// Serialize mints for a specific group.
    pub fn serialize_group(&self, group_idx: usize) -> Vec<u8> {
        let group = self.get_group(group_idx);
        let mut data = Vec::with_capacity(group.len() * PM_DA_PENDING_MINT_SIZE);
        for mint in group {
            data.extend_from_slice(bytemuck::bytes_of(mint));
        }
        data
    }

    /// Iterate over groups with their indices.
    pub fn groups(&self) -> impl Iterator<Item = (usize, &[PendingMint])> {
        (0..self.num_groups()).map(move |i| (i, self.get_group(i)))
    }

    /// Get all mints.
    pub fn all_mints(&self) -> &[PendingMint] {
        &self.mints
    }
}

/// Derive the pending mint buffer PDA.
pub fn derive_pending_mint_buffer_pda(program_id: &Pubkey, writer: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"mint_buffer", writer.as_ref()], program_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_mint(idx: u8) -> PendingMint {
        PendingMint {
            recipient: [idx; 32],
            amount: idx as u64 * 1000,
        }
    }

    #[test]
    fn test_builder_empty() {
        let builder = PendingMintBufferBuilder::new(vec![]);
        assert_eq!(builder.total_mints(), 0);
        assert_eq!(builder.num_groups(), 0);
        assert_eq!(builder.buffer_size(), PENDING_MINT_BUFFER_HEADER_SIZE);
    }

    #[test]
    fn test_builder_single_group() {
        let mints: Vec<PendingMint> = (0..10).map(create_test_mint).collect();
        let builder = PendingMintBufferBuilder::new(mints);

        assert_eq!(builder.total_mints(), 10);
        assert_eq!(builder.num_groups(), 1);
        assert_eq!(builder.get_group(0).len(), 10);
    }

    #[test]
    fn test_builder_multiple_groups() {
        let mints: Vec<PendingMint> = (0..50).map(|i| create_test_mint(i as u8)).collect();
        let builder = PendingMintBufferBuilder::new(mints);

        assert_eq!(builder.total_mints(), 50);
        // 50 / 24 = 2.08, so 3 groups
        assert_eq!(builder.num_groups(), 3);
        assert_eq!(builder.get_group(0).len(), PM_MAX_PENDING_MINTS_PER_GROUP);
        assert_eq!(builder.get_group(1).len(), PM_MAX_PENDING_MINTS_PER_GROUP);
        assert_eq!(builder.get_group(2).len(), 50 - 2 * PM_MAX_PENDING_MINTS_PER_GROUP);
    }

    #[test]
    fn test_serialize_group() {
        let mints: Vec<PendingMint> = (0..5).map(create_test_mint).collect();
        let builder = PendingMintBufferBuilder::new(mints);

        let data = builder.serialize_group(0);
        assert_eq!(data.len(), 5 * PM_DA_PENDING_MINT_SIZE);
    }
}
