use psy_bridge_core::{common_types::QHash256, crypto::hash::sha256_impl::hash_impl_sha256_bytes};
use psy_doge_solana_core::data_accounts::pending_mint::{PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH, PM_DA_PENDING_MINT_SIZE, PendingMint};


const MAX_PENDING_MINTS_PER_GROUP: usize = 24;
/*
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct PendingMint {
    pub recipient: [u8; 32],
    pub amount: u64,
}
*/
const PENDING_MINT_SIZE: usize = PM_DA_PENDING_MINT_SIZE;

pub struct PendingMintsGroupsBuilder {
    pub group_hashes_buffer: Vec<u8>,
    pub current_group: Vec<u8>,
    pub next_item_in_group_index: usize,
    pub total_groups: usize,
}

impl PendingMintsGroupsBuilder {
    pub fn new() -> Self {
        Self {
            group_hashes_buffer: vec![0u8; 2],
            current_group: vec![0u8; MAX_PENDING_MINTS_PER_GROUP * PENDING_MINT_SIZE],
            next_item_in_group_index: 0,
            total_groups: 0,
        }
    }
    pub fn new_with_hint(total_items_hint: usize) -> Self {
        let mut total_groups_hint = total_items_hint / MAX_PENDING_MINTS_PER_GROUP;
        if (total_groups_hint * MAX_PENDING_MINTS_PER_GROUP) < total_items_hint {
            total_groups_hint += 1;
        }
        let mut group_hashes_buffer = Vec::with_capacity(2 + total_groups_hint * 32);
        group_hashes_buffer.extend_from_slice(&[0u8; 2]);


        Self {
            group_hashes_buffer: group_hashes_buffer,
            current_group: vec![0u8; MAX_PENDING_MINTS_PER_GROUP * PENDING_MINT_SIZE],
            next_item_in_group_index: 0,
            total_groups: 0,
        }
    }

    pub fn append_pending_mint(&mut self, recipient_solana_public_key: &[u8; 32], amount: u64)  {

        let amount_bytes = amount.to_le_bytes();
        if self.next_item_in_group_index == MAX_PENDING_MINTS_PER_GROUP {
            let hash = hash_impl_sha256_bytes(&self.current_group[..MAX_PENDING_MINTS_PER_GROUP * PENDING_MINT_SIZE]);
            self.group_hashes_buffer.extend_from_slice(&hash);
            self.next_item_in_group_index = 0;
            self.total_groups += 1;
        }


        self.current_group[self.next_item_in_group_index * PENDING_MINT_SIZE..self.next_item_in_group_index * PENDING_MINT_SIZE + 32]
            .copy_from_slice(recipient_solana_public_key);
        self.current_group[self.next_item_in_group_index * PENDING_MINT_SIZE + 32..self.next_item_in_group_index * PENDING_MINT_SIZE + 40]
            .copy_from_slice(&amount_bytes);
        self.next_item_in_group_index += 1;
    }

    pub fn finalize(&mut self) -> anyhow::Result<QHash256> {
        if self.total_groups != 0 || self.next_item_in_group_index > 0 {
            let total_items = self.total_groups * MAX_PENDING_MINTS_PER_GROUP + self.next_item_in_group_index;
            if self.next_item_in_group_index > 0 {
                let hash = hash_impl_sha256_bytes(&self.current_group[..self.next_item_in_group_index * PENDING_MINT_SIZE]);
                self.group_hashes_buffer.extend_from_slice(&hash);
                self.total_groups += 1;
            }
            if total_items > u16::MAX as usize {
                return Err(anyhow::anyhow!("Too many pending mints: {}", total_items));
            }
            let total_items_u16 = total_items as u16;
            self.group_hashes_buffer[0..2].copy_from_slice(&total_items_u16.to_le_bytes());
        }
        Ok(hash_impl_sha256_bytes(&self.group_hashes_buffer))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(C)]
pub struct PendingMintsAutoClaimBufferTemplate {
    pub groups: Vec<Vec<u8>>,
    pub finalized_hash: QHash256,
    pub total_items: u16,
}

fn pending_mints_data_to_vec(data: &[u8]) -> anyhow::Result<Vec<PendingMint>> {
    if data.len() % PENDING_MINT_SIZE != 0 {
        return Err(anyhow::anyhow!("Invalid pending mints data length: {}", data.len()));
    }
    let mut pending_mints = Vec::new();
    let num_items = data.len() / PENDING_MINT_SIZE;
    for i in 0..num_items {
        let start_index = i * PENDING_MINT_SIZE;
        let mut recipient = [0u8; 32];
        recipient.copy_from_slice(&data[start_index..start_index + 32]);
        let amount = u64::from_le_bytes(
            data[start_index + 32..start_index + 40]
                .try_into()
                .unwrap(),
        );
        pending_mints.push(PendingMint {
            recipient,
            amount,
        });
    }
    Ok(pending_mints)
}
impl PendingMintsAutoClaimBufferTemplate {
    pub fn new_empty() -> Self {
        Self {
            groups: vec![],
            finalized_hash: PM_DA_DEFAULT_PENDING_MINTS_BUFFER_HASH,
            total_items: 0,
        }
    }
    pub fn get_pending_mints(&self) -> anyhow::Result<Vec<Vec<PendingMint>>> {
        self.groups.iter().map(|group_data| pending_mints_data_to_vec(group_data)).collect()
    }
}

pub struct PendingMintsGroupsBufferBuilder {
    pub groups_builder: PendingMintsGroupsBuilder,
    pub groups: Vec<Vec<u8>>,
}

impl PendingMintsGroupsBufferBuilder {
    pub fn new() -> Self {
        Self {
            groups_builder: PendingMintsGroupsBuilder::new(),
            groups: Vec::new(),
        }
    }
    pub fn new_with_hint(total_items_hint: usize) -> Self {
        Self {
            groups_builder: PendingMintsGroupsBuilder::new_with_hint(total_items_hint),
            groups: Vec::new(),
        }
    }

    pub fn append_pending_mint(&mut self, recipient_solana_public_key: &[u8; 32], amount: u64)  {
        if self.groups_builder.next_item_in_group_index == MAX_PENDING_MINTS_PER_GROUP {
            self.groups.push(self.groups_builder.current_group.clone());
        }
        self.groups_builder.append_pending_mint(recipient_solana_public_key, amount);
    }

    pub fn finalize(mut self) -> anyhow::Result<PendingMintsAutoClaimBufferTemplate> {
        let total_items = self.groups_builder.total_groups * MAX_PENDING_MINTS_PER_GROUP + self.groups_builder.next_item_in_group_index;
        if self.groups_builder.total_groups != 0 || self.groups_builder.next_item_in_group_index > 0 {
            if self.groups_builder.next_item_in_group_index > 0 {
                self.groups.push(self.groups_builder.current_group[..self.groups_builder.next_item_in_group_index * PENDING_MINT_SIZE].to_vec());
            }
        }
        let hash = self.groups_builder.finalize()?;

        Ok(PendingMintsAutoClaimBufferTemplate {
            groups: self.groups,
            finalized_hash: hash,
            total_items: total_items as u16,
        })
    }
}