use std::collections::HashMap;

use psy_bridge_core::{common_types::QHash256, crypto::hash::{merkle::utils::compute_root_merkle_proof_generic, sha256::SHA256_ZERO_HASHES, sha256_impl::hash_impl_sha256_bytes}, txo_constants::{TXO_BLOCK_FULL_MERKLE_TREE_HEIGHT, TXO_EMPTY_BLOCK_MERKLE_TREE_ROOT, TXO_FULL_MERKLE_TREE_HEIGHT, TXO_TREE_INDEX_BITS_BLOCK_NUM_LENGTH, TXO_TREE_INDEX_BITS_TX_NUM_LENGTH, TXO_TREE_LEAF_BIT_INDEX_LENGTH, TXO_TREE_MAX_OUTPUTS_PER_TX}};
use psy_doge_solana_core::data_accounts::pending_mint::PM_TXO_DEFAULT_BUFFER_HASH;

use crate::tree::{merkle_node::{SimpleMerkleNode, SimpleMerkleNodeKey}, sha256::CoreSha256Hasher, simple_merkle_tree_recorder::SimpleMemoryMerkleRecorderStore};
use psy_bridge_core::crypto::hash::traits::ZeroableHash;

#[inline(always)]
pub const fn get_bit_in_bit_vector(bit_vector: &QHash256, bit_index: u8) -> bool {
    bit_vector[ (bit_index >> 3) as usize ] & (1 << (bit_index & 0x07)) != 0
}

#[inline]
pub fn set_bit_in_bit_vector(bit_vector: &mut QHash256, bit_index: u8) {
    bit_vector[ (bit_index >> 3) as usize ] |= 1 << (bit_index & 0x07);
}

pub fn set_bit_in_bit_vector_cloned(bit_vector: &QHash256, bit_index: u8) -> QHash256 {
    let mut new_bit_vector = *bit_vector;
    set_bit_in_bit_vector(&mut new_bit_vector, bit_index);
    new_bit_vector
}





pub struct TxoLeafBuilder {
    pub leaf_map: HashMap<u64, QHash256>,
    pub txo_output_list_builder: TxBitBufferBuilder,
}

fn get_leaf_index_local_in_block(tx_index: u32, output_index: u32) -> u64 {
    1 << (TXO_TREE_INDEX_BITS_TX_NUM_LENGTH as u64) * (tx_index as u64) + (output_index as u64)
        >> TXO_TREE_LEAF_BIT_INDEX_LENGTH
}
fn get_leaf_bit_index_in_leaf(output_index: u32) -> u8 {
    ((output_index) & ((1 << TXO_TREE_LEAF_BIT_INDEX_LENGTH) - 1)) as u8
}

impl TxoLeafBuilder {
    pub fn new() -> Self {
        Self {
            leaf_map: HashMap::new(),
            txo_output_list_builder: TxBitBufferBuilder::new(),
        }
    }

    pub fn set_leaf_bit_to_true(&mut self, tx_index: u32, output_index: u32) {
        self.txo_output_list_builder
            .append_output(tx_index, output_index);
        let leaf_index = get_leaf_index_local_in_block(tx_index, output_index);
        let bit_index = get_leaf_bit_index_in_leaf(output_index);
        let leaf = {
            match self.leaf_map.get(&leaf_index) {
                Some(existing_hash) => *existing_hash,
                None => QHash256::get_zero_value(),
            }
        };
        let result = set_bit_in_bit_vector_cloned(&leaf, bit_index);

        self.leaf_map.insert(leaf_index, result);
    }

    pub fn finalize(
        self,
        block_number: u32,
        txo_tree_block_siblings: &[QHash256],
        //last_txo_root: Hash256,
    ) -> anyhow::Result<TxoLeafBuilderResult> {
        let base_leaf_index = (block_number as u64) << (TXO_BLOCK_FULL_MERKLE_TREE_HEIGHT);
        let mut tree = SimpleMemoryMerkleRecorderStore::<CoreSha256Hasher, QHash256>::new(
            TXO_FULL_MERKLE_TREE_HEIGHT as u8,
        );
        let last_tree_root_computed = compute_root_merkle_proof_generic::<QHash256, CoreSha256Hasher>(
            TXO_EMPTY_BLOCK_MERKLE_TREE_ROOT,
            block_number as u64,
            &txo_tree_block_siblings,
        );
        /*
        if last_tree_root_computed != last_txo_root {
            anyhow::bail!("Last inserted txo tree root does not match expected value");
        }*/

        let new_tree_block_tree_key = SimpleMerkleNodeKey {
            index: block_number as u64,
            level: TXO_TREE_INDEX_BITS_BLOCK_NUM_LENGTH as u8,
        };
        let sibling_keys = new_tree_block_tree_key.get_siblings_keys_to_height(0);
        for (k, v) in sibling_keys.iter().zip(txo_tree_block_siblings.iter()) {
            tree.set_node_value(*k, *v);
        }
        tree.commit_changes();
        let computed_tree_root = tree.get_root();
        if computed_tree_root != last_tree_root_computed {
            anyhow::bail!("Last inserted txo tree root does not match expected merkle tree root after inserting siblings");
        }

        for (leaf_index_offset, leaf_hash) in &self.leaf_map {
            let full_leaf_index = base_leaf_index + leaf_index_offset;
            tree.set_leaf(full_leaf_index, *leaf_hash);
        }

        let next_tree_block_tree_key = SimpleMerkleNodeKey {
            index: block_number as u64 + 1,
            level: TXO_TREE_INDEX_BITS_BLOCK_NUM_LENGTH as u8,
        };
        let next_tree_block_siblings =
            next_tree_block_tree_key.get_siblings_keys_to_height(0).iter().map(|k| tree.get_node_value(k)).collect::<Vec<_>>();



        let changes = tree.get_changes();
        let mut nodes = Vec::with_capacity(changes.len());
        for (key, value) in changes {
            nodes.push(SimpleMerkleNode {
                key: *key,
                value: *value,
            });
        }
        let txo_list_finalized_hash = self.txo_output_list_builder.get_hash();

        Ok(
            TxoLeafBuilderResult {
                merkle_nodes: nodes,
                txo_output_list: self.txo_output_list_builder.outputs,
                txo_output_list_finalized_hash: txo_list_finalized_hash,
                next_block_txo_tree_block_siblings: next_tree_block_siblings,
            }
        )
    }
}


pub struct TxBitBufferBuilder {
    pub outputs: Vec<u32>,
}

impl TxBitBufferBuilder {
    pub fn new() -> Self {
        Self {
            outputs: Vec::new(),
        }
    }
    pub fn new_with_total_outputs_hint(total_outputs_hint: usize) -> Self {
        Self {
            outputs: Vec::with_capacity(total_outputs_hint),
        }
    }
    pub fn append_output(&mut self, transaction_index: u32, output_in_tx_index: u32) {
        self.outputs.push(transaction_index * TXO_TREE_MAX_OUTPUTS_PER_TX as u32 + output_in_tx_index);
    }
    pub fn get_hash(&self) -> QHash256 {
        hash_impl_sha256_bytes(&bytemuck::cast_slice(&self.outputs))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TxoLeafBuilderResult {
    pub merkle_nodes: Vec<SimpleMerkleNode<QHash256>>,
    pub next_block_txo_tree_block_siblings: Vec<QHash256>,
    pub txo_output_list: Vec<u32>,
    pub txo_output_list_finalized_hash: QHash256,
}
impl TxoLeafBuilderResult {
    pub fn new_genesis() -> Self {
        Self {
            merkle_nodes: vec![],
            next_block_txo_tree_block_siblings: (0..TXO_TREE_INDEX_BITS_BLOCK_NUM_LENGTH)
                .map(|i| SHA256_ZERO_HASHES[i + TXO_BLOCK_FULL_MERKLE_TREE_HEIGHT ])
                .collect(),
            txo_output_list: vec![],
            txo_output_list_finalized_hash: PM_TXO_DEFAULT_BUFFER_HASH,
        }
    }
}