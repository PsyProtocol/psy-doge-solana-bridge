use psy_bridge_core::{
    common_types::{QHash160, QHash256}, crypto::hash::{
        merkle::fixed_append_tree::{
            FixedMerkleAppendTree, FixedMerkleAppendTreePartialMerkleProof,
        },
        sha256_impl::{
            hash_impl_sha256_bytes, hash_impl_sha256_two_to_one_bytes, hashv_impl_sha256_bytes
        },
    }, error::{DogeBridgeError, QDogeResult}, header::{PsyBridgeHeader, PsyBridgeStateCommitment}, txo_constants::get_txo_block_number_tx_number_output_index_from_combined_index
};

use crate::{ instructions::doge_bridge::InitializeBridgeInstructionData, program_state::{FinalizedBlockMintTxoManager, PsyReturnTxOutput, PsyWithdrawalChainSnapshot, PsyWithdrawalRequest}, utils::{deposit_leaf::hash_deposit_leaf, fees::{calcuate_deposit_fee, calcuate_withdrawal_fee}}};

const INVALID_BLOCK_HEIGHT: u32 = 0xFFFFFFFF;
const MIN_WAIT_TIME_REPLAY_WITHDRAWAL_SECS: u32 = 60; // 1 minute

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct PsyBridgeAccessControlHeader {
    pub operator_pubkey: [u8; 32],
    pub fee_spender_pubkey: [u8; 32],
}
#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct PsyBridgeConfig {
    pub deposit_fee_rate_numerator: u64,
    pub deposit_fee_rate_denominator: u64,
    pub withdrawal_fee_rate_numerator: u64,
    pub withdrawal_fee_rate_denominator: u64,
    pub deposit_flat_fee_sats: u64,
    pub withdrawal_flat_fee_sats: u64,
}
impl PsyBridgeConfig {
    pub fn get_hash(&self) -> QHash256 {
        hash_impl_sha256_bytes(bytemuck::bytes_of(self))
    }
}

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct PsyBridgeProgramState {
    pub bridge_header: PsyBridgeHeader,
    pub recent_finalized_blocks: [PsyBridgeStateCommitment; 8],
    pub last_return_output: PsyReturnTxOutput,
    pub pending_mint_txos: FinalizedBlockMintTxoManager,
    pub spent_txo_tree_root: QHash256,
    pub withdrawal_snapshot: PsyWithdrawalChainSnapshot,
    pub next_processed_withdrawals_index: u64,
    // tree whose leaves are the tx sighashes of sent transactions
    pub sent_transactions_tree: FixedMerkleAppendTree,
    pub manual_deposits_tree: FixedMerkleAppendTree,
    pub requested_withdrawals_tree: FixedMerkleAppendTree,

    pub bridge_doge_public_key_hash: QHash160,
    pub bridge_control_mode: u32,
    pub next_recent_finalized_block_index: u64,
    pub last_processed_withdrawals_at_ms: u64,
    pub total_requested_withdrawals_sats: u64,
    pub total_fees_withdrawn_sats: u64,
    pub total_manual_deposit_fees_sats: u64,
    pub total_withdrawal_fees_sats: u64,
    pub last_received_block_at_ms: u64,
    pub last_replayed_withdrawal_at_ms: u64,

    pub config_params: PsyBridgeConfig,

    pub access_control: PsyBridgeAccessControlHeader,
}

impl PsyBridgeProgramState {
    pub fn initialize(&mut self, initialize_instruction: &InitializeBridgeInstructionData) {
        self.recent_finalized_blocks = [initialize_instruction.bridge_header.finalized_state; 8];
        self.bridge_header.copy_from(&initialize_instruction.bridge_header);
        self.last_return_output = initialize_instruction.start_return_txo_output;
        self.pending_mint_txos = FinalizedBlockMintTxoManager::default();
        self.spent_txo_tree_root = QHash256::default();
        self.withdrawal_snapshot = PsyWithdrawalChainSnapshot::default();
        self.next_processed_withdrawals_index = 0;
        self.sent_transactions_tree = FixedMerkleAppendTree::new_empty();
        self.manual_deposits_tree = FixedMerkleAppendTree::new_empty();
        self.requested_withdrawals_tree = FixedMerkleAppendTree::new_empty();

        self.bridge_doge_public_key_hash = QHash160::default();
        self.bridge_control_mode = 0;
        self.next_recent_finalized_block_index = 0;
        self.last_processed_withdrawals_at_ms = 0;
        self.total_requested_withdrawals_sats = 0;
        self.total_fees_withdrawn_sats = 0;
        self.total_manual_deposit_fees_sats = 0;
        self.total_withdrawal_fees_sats = 0;
        self.last_received_block_at_ms = 0;
        self.last_replayed_withdrawal_at_ms = 0;
        self.config_params = initialize_instruction.config_params;
        self.access_control = PsyBridgeAccessControlHeader {
            operator_pubkey: initialize_instruction.operator_pubkey,
            fee_spender_pubkey: initialize_instruction.fee_spender_pubkey,
        };
    }
    pub fn get_total_finalized_fees(&self) -> u64 {
        self.total_manual_deposit_fees_sats + self.total_withdrawal_fees_sats + self.bridge_header.total_finalized_fees_collected_chain_history
    }
    pub fn get_operator_withdrawable_fees(&self) -> u64 {
        self.get_total_finalized_fees() - self.total_fees_withdrawn_sats
    }
    pub fn is_paused(&self, current_unix_timestamp_secs: u32) -> bool {
        self.bridge_header.is_paused(current_unix_timestamp_secs)
    }
    pub fn snapshot_for_withdrawal(&mut self, current_unix_timestamp_secs: u32) {
        self.withdrawal_snapshot = PsyWithdrawalChainSnapshot {
            auto_claimed_deposits_tree_root: self
                .bridge_header
                .finalized_state
                .auto_claimed_deposits_tree_root,
            requested_withdrawals_tree_root: self.requested_withdrawals_tree.get_root(),
            block_merkle_tree_root: self.bridge_header.finalized_state.block_merkle_tree_root,
            block_height: self.bridge_header.finalized_state.block_height,
            last_snapshotted_for_withdrawals_seconds: current_unix_timestamp_secs,
            next_requested_withdrawals_tree_index: self.requested_withdrawals_tree.next_index,
        };
    }
    pub fn get_expected_public_inputs_for_withdrawal_proof(
        &self,
        dogecoin_tx_sighash: &QHash256,
        new_return_output: &PsyReturnTxOutput,
        new_spent_txo_tree_root: QHash256,
        new_next_processed_withdrawals_index: u64,
    ) -> QHash256 {
        let snapshot_hash = self.withdrawal_snapshot.get_hash();
        let new_return_output_hash = new_return_output.get_hash();
        let old_return_output_hash = self.last_return_output.get_hash();
        let old_spent_txo_tree_root = self.spent_txo_tree_root;

        let return_output_hash_transition =
            hash_impl_sha256_two_to_one_bytes(&old_return_output_hash, &new_return_output_hash);
        let spent_txo_tree_root_transition =
            hash_impl_sha256_two_to_one_bytes(&old_spent_txo_tree_root, &new_spent_txo_tree_root);

        hashv_impl_sha256_bytes(
            &[dogecoin_tx_sighash,
            &snapshot_hash,
            &return_output_hash_transition,
            &spent_txo_tree_root_transition,
            &new_next_processed_withdrawals_index.to_le_bytes(),]
        )
    }
    pub fn process_request_withdrawal(
        &mut self,
        address_type: u32,
        address: [u8; 20],
        amount_burned_sats: u64,
    ) -> bool {
        let (amount_sats, fee_sats) =
            calcuate_withdrawal_fee(amount_burned_sats, self.config_params.withdrawal_flat_fee_sats as u64, self.config_params.withdrawal_fee_rate_numerator, self.config_params.withdrawal_fee_rate_denominator).unwrap_or((0,0));
        if fee_sats == 0 || amount_sats == 0 {
            return false;
        }
        self.total_withdrawal_fees_sats += fee_sats;

        let withdrawal_request = PsyWithdrawalRequest::new(address, amount_sats, address_type);
        let leaf = withdrawal_request.to_leaf();
        self.requested_withdrawals_tree.append(leaf);
        self.total_requested_withdrawals_sats += amount_sats;
        true

    }
    pub fn update_for_withdrawal(
        &mut self,
        new_return_output: PsyReturnTxOutput,
        new_spent_txo_tree_root: QHash256,
        new_next_processed_withdrawals_index: u64,
    ) {
        self.last_return_output = new_return_output;
        self.spent_txo_tree_root = new_spent_txo_tree_root;
        self.next_processed_withdrawals_index = new_next_processed_withdrawals_index;
        self.sent_transactions_tree
            .append(new_return_output.sighash);
    }
    pub fn process_replay_withdrawal_proof(
        &mut self,
        proof: &FixedMerkleAppendTreePartialMerkleProof,
        current_unix_timestamp_secs: u32,
    ) -> bool {
        let proof_root = proof.compute_root_sha256();
        if proof_root != self.sent_transactions_tree.get_root() {
            return false;
        }

        if self.last_processed_withdrawals_at_ms / 1000
            + (MIN_WAIT_TIME_REPLAY_WITHDRAWAL_SECS as u64)
            > current_unix_timestamp_secs as u64
        {
            return false;
        }
        self.last_processed_withdrawals_at_ms = current_unix_timestamp_secs as u64 * 1000;
        true
    }
    pub fn find_recent_auto_claim_txo_tree_root(&self, recent_auto_claim_txo_root: QHash256) -> u32 {
        if self.bridge_header.finalized_state.auto_claimed_deposits_tree_root
            == recent_auto_claim_txo_root
        {
            return self.bridge_header.finalized_state.block_height;
        }
        for i in 0..self.recent_finalized_blocks.len() {
            if self.recent_finalized_blocks[i].auto_claimed_deposits_tree_root
                == recent_auto_claim_txo_root
            {
                return self.recent_finalized_blocks[i].block_height;
            }
        }
        INVALID_BLOCK_HEIGHT

    }
    pub fn find_recent_auto_claim_txo_tree_root_and_block_merkle_root(&self, recent_auto_claim_txo_root: QHash256) -> (u32, QHash256) {
        if self.bridge_header.finalized_state.auto_claimed_deposits_tree_root
            == recent_auto_claim_txo_root
        {
            return (self.bridge_header.finalized_state.block_height, self.bridge_header.finalized_state.block_merkle_tree_root);
        }
        for i in 0..self.recent_finalized_blocks.len() {
            if self.recent_finalized_blocks[i].auto_claimed_deposits_tree_root
                == recent_auto_claim_txo_root
            {
                return (self.recent_finalized_blocks[i].block_height, self.recent_finalized_blocks[i].block_merkle_tree_root);
            }
        }
        (INVALID_BLOCK_HEIGHT, QHash256::default())

    }
    pub fn process_manual_claimed_deposit(
        &mut self,
        tx_hash: QHash256,
        recent_block_merkle_tree_root: QHash256,
        recent_auto_claim_txo_root: QHash256,
        combined_txo_index: u64,
        depositor_public_key: &[u8; 32],
        deposit_amount_sats: u64,
    ) -> QDogeResult<u64> {
        let (block_height, _, _) = get_txo_block_number_tx_number_output_index_from_combined_index(combined_txo_index);
        let (block_height_for_auto_claim_txo_root, block_merkle_tree_root) =
            self.find_recent_auto_claim_txo_tree_root_and_block_merkle_root(recent_auto_claim_txo_root);

        if block_height_for_auto_claim_txo_root == INVALID_BLOCK_HEIGHT || block_height > block_height_for_auto_claim_txo_root {
            return Err(DogeBridgeError::AutoClaimedDepositTreeRootNotRecentEnough);
        }
        if block_merkle_tree_root != recent_block_merkle_tree_root {
            return Err(DogeBridgeError::BlockMerkleTreeRootNotRecentEnough);
        }
        let (amount_sats, fee) = calcuate_deposit_fee(deposit_amount_sats, self.config_params.deposit_flat_fee_sats as u64, self.config_params.deposit_fee_rate_numerator, self.config_params.deposit_fee_rate_denominator).unwrap_or((0,0));
        if fee == 0 || amount_sats == 0 {
            return Err(DogeBridgeError::InsufficientBridgeFees);
        }

        let leaf_hash = hash_deposit_leaf(
            &tx_hash,
            combined_txo_index,
            depositor_public_key,
            amount_sats,
        );
        self.total_manual_deposit_fees_sats += fee;
        self.manual_deposits_tree.append(leaf_hash);

        Ok(amount_sats)
    }
}
