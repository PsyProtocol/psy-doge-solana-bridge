use psy_bridge_core::{common_types::QHash256, crypto::hash::{merkle::utils::compute_root_merkle_proof_generic, sha256::SHA256_ZERO_HASHES}, txo_constants::{TXO_TREE_INDEX_BITS_BLOCK_NUM_LENGTH, TXO_TREE_MAX_OUTPUTS_PER_TX, TXO_TREE_MAX_TX_PER_BLOCK, get_txo_combined_index}};
use psy_doge_solana_core::utils::{deposit_leaf::hash_deposit_leaf, fees::calcuate_deposit_fee};

use crate::{builders::pending_mints_buffer_builder::{PendingMintsAutoClaimBufferTemplate, PendingMintsGroupsBufferBuilder}, constants::{AUTO_CLAIM_DEPOSITS_TREE_EMPTY_ROOT, AUTO_CLAIM_DEPOSITS_TREE_HEIGHT}, tree::{merkle_node::{SimpleMerkleNode, SimpleMerkleNodeKey}, sha256::CoreSha256Hasher, simple_merkle_tree_recorder::SimpleMemoryMerkleRecorderStore}};




#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[repr(C)]
pub struct AutoClaimDepositsResult {
    pub start_deposits_root: QHash256,
    pub end_deposits_root: QHash256,
    pub merkle_nodes: Vec<SimpleMerkleNode<QHash256>>,
    pub pending_mints: PendingMintsAutoClaimBufferTemplate,
    pub total_mints: u32,
    pub total_auto_claimed_deposits: u32,
    pub total_deposit_amount: u64,
    pub next_auto_claimed_deposits_siblings: Vec<QHash256>,
    pub next_claim_deposits_index: u32,
    pub total_fees_collected: u64,
}
impl AutoClaimDepositsResult {
    pub fn new_genesis() -> Self {
        Self {
            start_deposits_root: AUTO_CLAIM_DEPOSITS_TREE_EMPTY_ROOT,
            end_deposits_root: AUTO_CLAIM_DEPOSITS_TREE_EMPTY_ROOT,
            merkle_nodes: vec![],
            pending_mints: PendingMintsAutoClaimBufferTemplate::new_empty(),
            total_mints: 0,
            total_auto_claimed_deposits: 0,
            total_deposit_amount: 0,
            next_auto_claimed_deposits_siblings: (0..AUTO_CLAIM_DEPOSITS_TREE_HEIGHT)
                .map(|i| SHA256_ZERO_HASHES[i])
                .collect(),
            next_claim_deposits_index: 0,
            total_fees_collected: 0,
        }
    }
}

pub struct AutoClaimDepositsBuilder {
    pub deposits_tree: SimpleMemoryMerkleRecorderStore<CoreSha256Hasher, QHash256>,
    pub start_root: QHash256,
    pub next_index: u64,
    pub pending_mints: PendingMintsGroupsBufferBuilder,
    pub total_mints: u32,
    pub total_deposit_amount: u64,
    pub flat_fee_per_deposit_sats: u64,
    pub deposit_fee_rate_numerator: u64,
    pub deposit_fee_rate_denominator: u64,
    pub total_fees_collected: u64,
}
impl AutoClaimDepositsBuilder {
    pub fn new(
        next_auto_claimed_deposits_siblings: &[QHash256],
        next_claim_deposits_index: u32,
        total_items_hint: usize,
        flat_fee_per_deposit_sats: u64,
        deposit_fee_rate_numerator: u64,
        deposit_fee_rate_denominator: u64,
    ) -> Self {
        let mut tree = SimpleMemoryMerkleRecorderStore::<CoreSha256Hasher, QHash256>::new(
            AUTO_CLAIM_DEPOSITS_TREE_HEIGHT as u8,
        );
        //let last_tree_root_computed = compute_root_merkle_proof_generic::<QHash256, CoreSha256Hasher>(*claim_deposits_last_value, claim_deposits_last_index as u64, &last_auto_claimed_deposits_siblings);
        let new_tree_block_tree_key = SimpleMerkleNodeKey {
            index: next_claim_deposits_index as u64,
            level: TXO_TREE_INDEX_BITS_BLOCK_NUM_LENGTH as u8,
        };
        let sibling_keys = new_tree_block_tree_key.get_siblings_keys_to_height(0);
        for (k, v) in sibling_keys
            .iter()
            .zip(next_auto_claimed_deposits_siblings.iter())
        {
            tree.set_node_value(*k, *v);
        }
        let start_root = compute_root_merkle_proof_generic::<QHash256, CoreSha256Hasher>([0u8; 32], next_claim_deposits_index as u64, &next_auto_claimed_deposits_siblings);
        tree.set_node_value(SimpleMerkleNodeKey::new_root(), start_root);
        tree.commit_changes();
        Self {
            deposits_tree: tree,
            start_root,
            next_index: next_claim_deposits_index as u64,
            pending_mints: PendingMintsGroupsBufferBuilder::new_with_hint(total_items_hint),
            total_mints: 0,
            total_deposit_amount: 0,
            total_fees_collected: 0,
            flat_fee_per_deposit_sats,
            deposit_fee_rate_numerator,
            deposit_fee_rate_denominator,
        }
    }

    pub fn add_auto_claim_deposit(
        &mut self,
        recipient_solana_public_key: QHash256,
        tx_hash: QHash256,
        block_height: u32,
        transaction_index: u32,
        output_index: u32,
        amount: u64,
    ) -> anyhow::Result<()> {
        let fee_result = calcuate_deposit_fee(
            amount,
            self.flat_fee_per_deposit_sats,
            self.deposit_fee_rate_numerator,
            self.deposit_fee_rate_denominator,
        )?;
        self.pending_mints
            .append_pending_mint(&recipient_solana_public_key, fee_result.amount_after_fees);
        if transaction_index > TXO_TREE_MAX_TX_PER_BLOCK as u32 {
            anyhow::bail!("Transaction index exceeds maximum allowed per block");
        }else if output_index > TXO_TREE_MAX_OUTPUTS_PER_TX as u32{
            anyhow::bail!("Output index exceeds maximum allowed per transaction");
        }
        let combined_index = get_txo_combined_index(block_height, transaction_index as u16, output_index as u16);
        let leaf_hash =
            hash_deposit_leaf(&tx_hash, combined_index, &recipient_solana_public_key, amount);
        self.deposits_tree
            .set_leaf(self.next_index, leaf_hash);
        self.next_index += 1;
        self.total_mints += 1;
        self.total_fees_collected = self
            .total_fees_collected
            .checked_add(fee_result.fees_generated)
            .ok_or_else(|| anyhow::anyhow!("Total fees collected overflow"))?;
        self.total_deposit_amount = self
            .total_deposit_amount
            .checked_add(amount)
            .ok_or_else(|| anyhow::anyhow!("Total deposit amount overflow"))?;
        Ok(())
    }
    pub fn get_changes_merkle_nodes(
        &self
    ) -> Vec<SimpleMerkleNode<QHash256>> {
        self.deposits_tree.get_changes_merkle_nodes()
    }
    pub fn finalize(self) -> anyhow::Result<AutoClaimDepositsResult> {
        let end_deposits_root = self.deposits_tree.get_root();

        let merkle_nodes = self.get_changes_merkle_nodes();
        let next_claim_deposits_index = self.next_index as u32;

        let next_auto_claimed_deposits_siblings = self.deposits_tree.get_siblings_for_leaf(
            self.next_index,
        );
        let pending_mints_buffer = self.pending_mints.finalize()?;
        Ok(AutoClaimDepositsResult {
            merkle_nodes,
            pending_mints: pending_mints_buffer,
            total_mints: self.total_mints,
            total_auto_claimed_deposits: self.total_mints,
            total_deposit_amount: self.total_deposit_amount,
            next_auto_claimed_deposits_siblings,
            next_claim_deposits_index,
            start_deposits_root: self.start_root,
            end_deposits_root,
            total_fees_collected: self.total_fees_collected,
        })
    }
}