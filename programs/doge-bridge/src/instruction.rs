use psy_bridge_core::header::PsyBridgeHeader;
use psy_bridge_core::crypto::zk::CompactBridgeZKProof;
use psy_doge_solana_core::instructions::doge_bridge::ProcessReorgBlocksFixedData;
use psy_doge_solana_core::program_state::FinalizedBlockMintTxoInfo;


// Helper for Reorg reading
pub struct ReorgBlockUpdateReader<'a> {
    pub proof: &'a CompactBridgeZKProof,
    pub header: &'a PsyBridgeHeader,
    pub extra_finalized_blocks: Vec<&'a FinalizedBlockMintTxoInfo>,
}

impl<'a> ReorgBlockUpdateReader<'a> {
    pub fn new(data: &'a [u8]) -> Option<Self> {
        let fixed_size = std::mem::size_of::<ProcessReorgBlocksFixedData>();
        if data.len() < fixed_size {
            return None;
        }
        let fixed_bytes = &data[..fixed_size];
        let fixed: &ProcessReorgBlocksFixedData = bytemuck::from_bytes(fixed_bytes);

        let item_size = std::mem::size_of::<FinalizedBlockMintTxoInfo>();
        let remaining_len = data.len() - fixed_size;
        
        let items = if remaining_len > 0 { remaining_len / item_size } else { 0 };
        let mut extra_blocks = Vec::with_capacity(items);
        for i in 0..items {
            let start = fixed_size + i * item_size;
            let item_bytes = &data[start..(start + item_size)];
            if item_bytes.len() != item_size { return None; }
            let item: &FinalizedBlockMintTxoInfo = bytemuck::from_bytes(item_bytes);
            extra_blocks.push(item);
        }

        Some(Self {
            proof: &fixed.proof,
            header: &fixed.header,
            extra_finalized_blocks: extra_blocks,
        })
    }
}