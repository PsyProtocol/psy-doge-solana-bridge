use psy_bridge_core::{common_types::QHash256, crypto::hash::sha256_impl::hashv_impl_sha256_bytes};


pub fn hash_deposit_leaf(
    tx_hash: &QHash256,
    combined_index: u64,
    depositor_public_key: &QHash256,
    amount: u64,
) -> QHash256 {
    hashv_impl_sha256_bytes(
        &[
            tx_hash,
            depositor_public_key,
            &combined_index.to_le_bytes(),
            &amount.to_le_bytes(),
        ],
    )
}