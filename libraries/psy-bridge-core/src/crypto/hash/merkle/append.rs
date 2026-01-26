use crate::{common_types::QHash256, crypto::hash::{sha256::SHA256_ZERO_HASHES, sha256_impl::hash_impl_sha256_two_to_one_bytes}};


pub fn update_siblings_append_merkle_tree(
    next_siblings: &mut [QHash256],
    next_value: QHash256,
    next_index: u64,
) -> QHash256{
    let next_siblings_len = next_siblings.len();
    let mut index = next_index;
    let mut current = next_value;
    let mut next_changes = next_index ^ (next_index + 1);
    for i in 0..next_siblings_len {
        if (index & 1) == 0 {
            // Left Child
            if (next_changes & 1) == 1 {
                next_siblings[i] = current;
            }
            current = hash_impl_sha256_two_to_one_bytes(&current, &SHA256_ZERO_HASHES[i]);
        } else {
            // Right Child
            current = hash_impl_sha256_two_to_one_bytes(&next_siblings[i], &current);
            if (next_changes & 1) == 1 {
                next_siblings[i] = SHA256_ZERO_HASHES[i];
            }
        }
        index >>= 1;
        next_changes >>= 1;
    }
    current
}
pub fn update_siblings_append_merkle_tree_imm_fixed<const N: usize>(
    next_siblings: &[QHash256; N],
    next_value: QHash256,
    next_index: u64,
) -> (QHash256, [QHash256; N]){
    let mut next_siblings: [QHash256; N] = *next_siblings;
    let root = update_siblings_append_merkle_tree(&mut next_siblings, next_value, next_index);
    (root, next_siblings)
}
