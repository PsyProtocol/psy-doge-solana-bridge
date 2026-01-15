use psy_bridge_core::custodian_config::BridgeCustodianWalletConfig;
use psy_bridge_core::{common_types::QHash256, header::PsyBridgeHeader};
use psy_bridge_core::crypto::zk::CompactBridgeZKProof;
use crate::program_state::{FinalizedBlockMintTxoInfo, PsyBridgeConfig, PsyReturnTxOutput, PsyWithdrawalRequest};

// Instruction Discriminators
pub const DOGE_BRIDGE_INSTRUCTION_INITIALIZE: u8 = 0;
pub const DOGE_BRIDGE_INSTRUCTION_BLOCK_UPDATE: u8 = 1;
pub const DOGE_BRIDGE_INSTRUCTION_REQUEST_WITHDRAWAL: u8 = 2;
pub const DOGE_BRIDGE_INSTRUCTION_PROCESS_WITHDRAWAL: u8 = 3;
pub const DOGE_BRIDGE_INSTRUCTION_OPERATOR_WITHDRAW_FEES: u8 = 4;
pub const DOGE_BRIDGE_INSTRUCTION_PROCESS_MANUAL_DEPOSIT: u8 = 5;
pub const DOGE_BRIDGE_INSTRUCTION_REPLAY_WITHDRAWAL: u8 = 6;
pub const DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP: u8 = 7;
pub const DOGE_BRIDGE_INSTRUCTION_PROCESS_REORG_BLOCKS: u8 = 8;
pub const DOGE_BRIDGE_INSTRUCTION_PROCESS_MINT_GROUP_AUTO_ADVANCE: u8 = 9;

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct InitializeBridgeParams {
    pub bridge_header: PsyBridgeHeader,
    pub start_return_txo_output: PsyReturnTxOutput,
    pub config_params: PsyBridgeConfig,
    pub custodian_wallet_config: BridgeCustodianWalletConfig,
}

// 0. Initialize
#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct InitializeBridgeInstructionData {
    pub operator_pubkey: [u8; 32],
    pub fee_spender_pubkey: [u8; 32],
    pub doge_mint: [u8; 32],
    pub bridge_header: PsyBridgeHeader,
    pub start_return_txo_output: PsyReturnTxOutput,
    pub config_params: PsyBridgeConfig,
    pub custodian_wallet_config: BridgeCustodianWalletConfig,
}

#[macro_rules_attribute::apply(crate::DeriveCopySerializeReprC)]
pub struct BlockUpdateFixedData {
    #[cfg_attr(feature = "serialize_serde", serde(with = "psy_bridge_core::serde_arrays::serde_arrays"))]
    pub proof: CompactBridgeZKProof, // 256 bytes
    pub header: PsyBridgeHeader,     // Fixed size struct
}
impl Default for BlockUpdateFixedData {
    fn default() -> Self {
        Self {
            proof: [0u8; 256],
            header: PsyBridgeHeader::default(),
        }
    }
}

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct RequestWithdrawalInstructionData {
    pub request: PsyWithdrawalRequest,
    pub recipient_address: [u8; 20],
    pub address_type: u32,
}

#[macro_rules_attribute::apply(crate::DeriveCopySerializeReprC)]
pub struct ProcessWithdrawalInstructionData {
    #[cfg_attr(feature = "serialize_serde", serde(with = "psy_bridge_core::serde_arrays::serde_arrays"))]
    pub proof: CompactBridgeZKProof,
    pub new_return_output: PsyReturnTxOutput,
    pub new_spent_txo_tree_root: QHash256,
    pub new_next_processed_withdrawals_index: u64,
}
impl Default for ProcessWithdrawalInstructionData {
    fn default() -> Self {
        Self {
            proof: [0u8; 256],
            new_return_output: PsyReturnTxOutput::default(),
            new_spent_txo_tree_root: [0u8; 32],
            new_next_processed_withdrawals_index: 0,
        }
    }
}

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct ProcessManualDepositInstructionData {
    pub tx_hash: QHash256,
    pub recent_block_merkle_tree_root: QHash256,
    pub recent_auto_claim_txo_root: QHash256,
    pub combined_txo_index: u64,
    pub depositor_solana_public_key: [u8; 32],
    pub deposit_amount_sats: u64,
}

// Process Reorg Blocks (Fixed Data Part)
// Followed by dynamic array of FinalizedBlockMintTxoInfo
#[macro_rules_attribute::apply(crate::DeriveCopySerializeReprC)]
pub struct ProcessReorgBlocksFixedData {
    #[cfg_attr(feature = "serialize_serde", serde(with = "psy_bridge_core::serde_arrays::serde_arrays"))]
    pub proof: CompactBridgeZKProof, 
    pub header: PsyBridgeHeader,
}
impl Default for ProcessReorgBlocksFixedData {
    fn default() -> Self {
        Self {
            proof: [0u8; 256],
            header: PsyBridgeHeader::default(),
        }
    }
}

// Helper to read BlockUpdate data
pub struct BlockUpdateReader<'a> {
    pub proof: &'a CompactBridgeZKProof,
    pub header: &'a PsyBridgeHeader,
    pub extra_finalized_blocks: Vec<&'a FinalizedBlockMintTxoInfo>,
}

impl<'a> BlockUpdateReader<'a> {
    pub fn new(data: &'a [u8]) -> Option<Self> {
        let fixed_size = std::mem::size_of::<BlockUpdateFixedData>();
        if data.len() < fixed_size {
            return None;
        }
        let fixed_bytes = &data[..fixed_size];
        let fixed: &BlockUpdateFixedData = bytemuck::from_bytes(fixed_bytes);

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