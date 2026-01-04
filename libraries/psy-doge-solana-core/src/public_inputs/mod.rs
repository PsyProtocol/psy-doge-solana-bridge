use psy_bridge_core::{common_types::{QHash160, QHash256}, crypto::hash::sha256_impl::{hash_impl_sha256_hash_four_buffers_concat, hash_impl_sha256_two_to_one_bytes, hashv_impl_sha256_bytes}};

use crate::program_state::FinalizedBlockMintTxoInfo;

pub fn get_withdrawal_proof_public_inputs(
    snapshot_hash: &QHash256,
    old_return_output_hash: &QHash256,
    new_return_output_hash: &QHash256,
    old_spent_txo_tree_root: &QHash256,
    new_spent_txo_tree_root: &QHash256,
    new_next_processed_withdrawals_index: u64,
) -> QHash256 {
    let return_output_hash_transition =
        hash_impl_sha256_two_to_one_bytes(old_return_output_hash, new_return_output_hash);
    let spent_txo_tree_root_transition =
        hash_impl_sha256_two_to_one_bytes(old_spent_txo_tree_root, new_spent_txo_tree_root);

    hash_impl_sha256_hash_four_buffers_concat(
        snapshot_hash,
        &return_output_hash_transition,
        &spent_txo_tree_root_transition,
        &new_next_processed_withdrawals_index.to_le_bytes(),
    )
}

pub fn get_block_transition_public_inputs(
    previous_header_hash: &QHash256,
    new_header_hash: &QHash256,
    config_params_hash: &QHash256,
    bridge_public_key_hash: &QHash160,
) -> QHash256 {

    let transition_hash = hash_impl_sha256_two_to_one_bytes(
        &previous_header_hash,
        &new_header_hash,
    );
    hashv_impl_sha256_bytes(
        &[
            &transition_hash,
            config_params_hash,
            bridge_public_key_hash,
        ]
    )
}


pub fn compute_backlog_hash(
    backlog_txo_mints: &[&FinalizedBlockMintTxoInfo],
) -> QHash256 {
    if backlog_txo_mints.is_empty() {
        return [0u8; 32];
    }
    
    let mut current_hash = [0u8; 32];
    for mint_info in backlog_txo_mints {
        let combined_hash = hash_impl_sha256_two_to_one_bytes(
            &mint_info.pending_mints_finalized_hash,
            &mint_info.txo_output_list_finalized_hash,
        );
        current_hash = hash_impl_sha256_two_to_one_bytes(&current_hash, &combined_hash);
    }
    current_hash
}

pub fn get_reorg_block_transition_public_inputs(
    previous_header_hash: &QHash256,
    new_header_hash: &QHash256,
    backlog_txo_mints: &[&FinalizedBlockMintTxoInfo],
    config_params_hash: &QHash256,
    bridge_public_key_hash: &QHash160,
) -> QHash256 {
    let backlog_hash = compute_backlog_hash(backlog_txo_mints);

    let transition_hash = hash_impl_sha256_two_to_one_bytes(
        &previous_header_hash,
        &new_header_hash,
    );

    let transition_and_backlog_hash = hash_impl_sha256_two_to_one_bytes(
        &transition_hash,
        &backlog_hash,
    );
    
    hashv_impl_sha256_bytes(&[
        &transition_and_backlog_hash,
        config_params_hash,
        bridge_public_key_hash
    ])
}

pub fn get_manual_deposit_proof_public_inputs(
    recent_block_merkle_tree_root: &QHash256,
    recent_auto_claim_txo_root: &QHash256,
    old_manual_claim_deposit_txo_root: &QHash256,
    new_manual_claim_txo_root: &QHash256,
    tx_hash: &QHash256,
    user_ata: &QHash256,
    combined_txo_index: u64,
    deposit_amount_sats: u64,
) -> QHash256 {
    let recent_info = hash_impl_sha256_two_to_one_bytes(recent_block_merkle_tree_root, recent_auto_claim_txo_root);
    let manual_claim_deposit_txo_transition = hash_impl_sha256_two_to_one_bytes(old_manual_claim_deposit_txo_root, new_manual_claim_txo_root);
    let recent_info_with_user_txo_root = hash_impl_sha256_two_to_one_bytes(&recent_info, &manual_claim_deposit_txo_transition);

    let tx_info = hashv_impl_sha256_bytes(&[
        tx_hash,
        user_ata,
        &combined_txo_index.to_le_bytes(),
        &deposit_amount_sats.to_le_bytes(),
    ]);

    hash_impl_sha256_two_to_one_bytes(&recent_info_with_user_txo_root, &tx_info)
}