use psy_bridge_core::common_types::QHash256;

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct PendingMint {
    pub recipient: [u8; 32],
    pub amount: u64,
}
pub const PM_DA_PENDING_MINT_SIZE: usize = std::mem::size_of::<PendingMint>();

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct PendingMintsBufferStateHeader {
    // Offset 0
    pub authorized_locker_public_key: [u8; 32],
    // Offset 32
    pub authorized_writer_public_key: [u8; 32],
    // Offset 64
    pub is_locked: u8,
    // Offset 65
    pub mode: u8,
    // Offset 66 (Aligned to 2)
    pub pending_mint_groups_count: u16,
    // Offset 68
    pub pending_mints_initialized: u16,
    // Offset 70
    pub pending_mints_count: u16,
}

pub const PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE: usize =
    std::mem::size_of::<PendingMintsBufferStateHeader>();
const _ASSERT_SIZE_PM_DA_PM: () = assert!(PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE == 72);

pub const PM_MAX_PENDING_MINTS_PER_GROUP: usize = 24;
pub const PM_MAX_PENDING_MINTS_PER_GROUP_U16: u16 = PM_MAX_PENDING_MINTS_PER_GROUP as u16;

pub fn pm_calculate_data_account_min_size(pending_mints_count: u16) -> usize {
    let groups = (pending_mints_count as usize + PM_MAX_PENDING_MINTS_PER_GROUP - 1)
        / PM_MAX_PENDING_MINTS_PER_GROUP;

    PM_DA_PENDING_MINTS_BUFFER_STATE_HEADER_SIZE
        + (groups * 32)
        + (pending_mints_count as usize * PM_DA_PENDING_MINT_SIZE)
}

// the hash of sha256([0u8; 2])
pub const PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH: QHash256 = [
    150, 162, 150, 210, 36, 242, 133, 198, 123, 238, 147, 195, 15, 138, 48, 145, 87, 240, 218, 163,
    93, 197, 184, 126, 65, 11, 120, 99, 10, 9, 207, 199,
];

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct PendingMintsTxoBufferHeader {
    // Offset 0
    pub authorized_writer: [u8; 32],
    // Offset 32
    pub init_status: u16,
    // Offset 34
    pub finalized_status: u16,
    // Offset 36
    pub doge_block_height: u32,
    // Offset 40
    pub batch_id: u32,
    // Offset 44
    pub data_size: u32,
    // Total Size: 48 bytes
}

pub const PM_TXO_BUFFER_HEADER_SIZE: usize = std::mem::size_of::<PendingMintsTxoBufferHeader>();
const _ASSERT_SIZE_PM_TXO: () = assert!(PM_TXO_BUFFER_HEADER_SIZE == 48);

// this should be sha256([]) (aka empty sha256)
pub const PM_TXO_DEFAULT_BUFFER_HASH: QHash256 = [
    227, 176, 196, 66, 152, 252, 28, 20, 154, 251, 244, 200, 153, 111, 185, 36, 39, 174, 65, 228,
    100, 155, 147, 76, 164, 149, 153, 27, 120, 82, 184, 85,
];

pub fn pm_txo_data_account_min_size(total_pending_mints: u16) -> usize {
    PM_TXO_BUFFER_HEADER_SIZE + (total_pending_mints as usize * 4)
}