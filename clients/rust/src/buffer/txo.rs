//! TXO buffer building utilities.
//!
//! Handles creation and population of TXO (transaction output) buffer accounts
//! for tracking spent outputs.

use solana_sdk::pubkey::Pubkey;

use super::CHUNK_SIZE;

/// Header size for TXO buffer.
pub const TXO_BUFFER_HEADER_SIZE: usize = 48;

/// Builder for TXO buffer data.
pub struct TxoBufferBuilder {
    indices: Vec<u32>,
    block_height: u32,
}

impl TxoBufferBuilder {
    /// Create a new builder with the given indices.
    pub fn new(indices: Vec<u32>, block_height: u32) -> Self {
        Self {
            indices,
            block_height,
        }
    }

    /// Get the block height.
    pub fn block_height(&self) -> u32 {
        self.block_height
    }

    /// Get the total number of indices.
    pub fn total_indices(&self) -> usize {
        self.indices.len()
    }

    /// Get the data size in bytes (excluding header).
    pub fn data_size(&self) -> usize {
        self.indices.len() * 4
    }

    /// Calculate the total buffer size needed.
    pub fn buffer_size(&self) -> usize {
        TXO_BUFFER_HEADER_SIZE + self.data_size()
    }

    /// Serialize all indices to bytes.
    pub fn serialize_all(&self) -> Vec<u8> {
        self.indices.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// Get the number of chunks needed for writing.
    pub fn num_chunks(&self) -> usize {
        let data_size = self.data_size();
        if data_size == 0 {
            0
        } else {
            (data_size + CHUNK_SIZE - 1) / CHUNK_SIZE
        }
    }

    /// Get a specific chunk of data.
    pub fn get_chunk(&self, chunk_idx: usize) -> Vec<u8> {
        let all_data = self.serialize_all();
        let start = chunk_idx * CHUNK_SIZE;
        let end = std::cmp::min(start + CHUNK_SIZE, all_data.len());
        all_data[start..end].to_vec()
    }

    /// Iterate over chunks with their offsets.
    pub fn chunks(&self) -> Vec<(usize, Vec<u8>)> {
        let all_data = self.serialize_all();
        all_data
            .chunks(CHUNK_SIZE)
            .enumerate()
            .map(|(i, chunk)| (i * CHUNK_SIZE, chunk.to_vec()))
            .collect()
    }

    /// Get all indices.
    pub fn all_indices(&self) -> &[u32] {
        &self.indices
    }
}

/// Derive the TXO buffer PDA.
pub fn derive_txo_buffer_pda(program_id: &Pubkey, writer: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"txo_buffer", writer.as_ref()], program_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_empty() {
        let builder = TxoBufferBuilder::new(vec![], 100);
        assert_eq!(builder.total_indices(), 0);
        assert_eq!(builder.data_size(), 0);
        assert_eq!(builder.num_chunks(), 0);
        assert_eq!(builder.block_height(), 100);
    }

    #[test]
    fn test_builder_small() {
        let indices: Vec<u32> = (0..10).collect();
        let builder = TxoBufferBuilder::new(indices, 100);

        assert_eq!(builder.total_indices(), 10);
        assert_eq!(builder.data_size(), 40);
        assert_eq!(builder.num_chunks(), 1);
    }

    #[test]
    fn test_builder_multiple_chunks() {
        // CHUNK_SIZE = 900, each u32 = 4 bytes
        // 900 / 4 = 225 indices per chunk
        let indices: Vec<u32> = (0..500).collect();
        let builder = TxoBufferBuilder::new(indices, 100);

        assert_eq!(builder.total_indices(), 500);
        assert_eq!(builder.data_size(), 2000);
        // 2000 / 900 = 2.22, so 3 chunks
        assert_eq!(builder.num_chunks(), 3);
    }

    #[test]
    fn test_serialize_all() {
        let indices = vec![1u32, 2, 3];
        let builder = TxoBufferBuilder::new(indices, 100);

        let data = builder.serialize_all();
        assert_eq!(data.len(), 12);
        assert_eq!(&data[0..4], &1u32.to_le_bytes());
        assert_eq!(&data[4..8], &2u32.to_le_bytes());
        assert_eq!(&data[8..12], &3u32.to_le_bytes());
    }

    #[test]
    fn test_chunks_iteration() {
        let indices: Vec<u32> = (0..300).collect();
        let builder = TxoBufferBuilder::new(indices, 100);

        let chunks: Vec<(usize, Vec<u8>)> = builder.chunks();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].0, 0); // First chunk at offset 0
        assert_eq!(chunks[1].0, CHUNK_SIZE); // Second chunk at CHUNK_SIZE
    }
}
