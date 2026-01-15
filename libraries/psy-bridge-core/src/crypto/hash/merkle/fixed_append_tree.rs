/*
Copyright (C) 2025 Zero Knowledge Labs Limited, Psy Protocol
... (License and Attribution preserved)
*/

use crate::{
    common_types::QHash256, crypto::hash::{merkle::{append::update_siblings_append_merkle_tree, delta_merkle_proof::DeltaMerkleProofCore, merkle_proof::{MerkleProofCore, MerkleProofCorePartial}}, sha256::{QSha256Hasher, SHA256_ZERO_HASHES}, sha256_impl::{hash_impl_sha256_compute_merkle_root, hash_impl_sha256_two_to_one_bytes}}, error::{DogeBridgeError, QDogeResult}
};

const TREE_HEIGHT: usize = 32;
type Hash = QHash256;

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct FixedMerkleAppendTreePartialMerkleProof {
    pub index: u64,
    pub value: Hash,
    pub siblings: [Hash; TREE_HEIGHT],
}
impl FixedMerkleAppendTreePartialMerkleProof {
    pub fn new_from_params(index: u64, value: Hash, siblings: [Hash; TREE_HEIGHT]) -> Self {
        Self {
            index,
            value,
            siblings: siblings,
        }
    }
    pub fn compute_root_sha256(&self) -> Hash {
        hash_impl_sha256_compute_merkle_root(&self.value, self.index, &self.siblings)
    }
}

pub fn get_changed_next_siblings_for_revert(
    current_next_index: u64,
    target_next_index: u64,
    target_next_siblings: &[Hash],
) -> QDogeResult<Vec<Hash>> {
    if current_next_index == 0 || target_next_index >= current_next_index {
         return Err(DogeBridgeError::RevertIndexTooHigh);
    }
    let mut changed_left_siblings = Vec::new();
    let mut temp_index = target_next_index;
    let mut changed_index = target_next_index ^ current_next_index;

    for i in 0..TREE_HEIGHT {
        if (temp_index & 1) != 0 && changed_index != 0 {
            // Right child: Sibling is stored Left.
            changed_left_siblings.push(target_next_siblings[i]);
        }
        temp_index >>= 1;
        changed_index >>= 1;
    }
    Ok(changed_left_siblings)
}

#[macro_rules_attribute::apply(crate::DeriveCopySerializeDefaultReprC)]
pub struct FixedMerkleAppendTreeDeltaPartialMerkleProof {
    pub new_value: Hash,
    pub old_value: Hash,
    pub index: u64,
    pub siblings: [Hash; TREE_HEIGHT],
}

#[macro_rules_attribute::apply(crate::DeriveCopySerializeReprC)]
pub struct FixedMerkleAppendTree {
    pub next_index: u64,
    pub current_root: Hash,
    pub next_siblings: [Hash; TREE_HEIGHT],
}
impl Default for FixedMerkleAppendTree {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl FixedMerkleAppendTree {
    
    // --- Constructors ---

    pub fn new_empty() -> Self {
        Self {
            next_index: 0,
            current_root: SHA256_ZERO_HASHES[TREE_HEIGHT],
            next_siblings: core::array::from_fn(|i| SHA256_ZERO_HASHES[i]),
        }
    }

    pub fn new_vec(
        next_index: u64,
        next_siblings: Vec<Hash>,
    ) -> Self {
        assert_eq!(next_siblings.len(), TREE_HEIGHT);
        
        let siblings_arr: [Hash; TREE_HEIGHT] = match next_siblings.try_into() {
            Ok(x) => x,
            Err(_) => panic!("Invalid siblings length"),
        };
        
        Self::new(next_index, siblings_arr)
    }

    pub fn new(
        next_index: u64,
        next_siblings: [Hash; TREE_HEIGHT],
    ) -> Self {
        if next_index == 0 {
            return Self::new_empty();
        }

        Self {
            next_index,
            current_root: hash_impl_sha256_compute_merkle_root(&[0u8; 32], next_index, &next_siblings),
            next_siblings,
        }
    }

    // --- Getters ---

    pub fn get_next_index(&self) -> u64 {
        self.next_index
    }

    pub fn get_height(&self) -> u8 {
        TREE_HEIGHT as u8
    }

    pub fn get_root(&self) -> Hash {
        self.current_root
    }

    pub fn get_value(&self) -> Hash {
        // If next_index is odd, we are waiting for a Right child, so the Left is stored in siblings[0].
        if (self.next_index & 1) == 1 {
            self.next_siblings[0]
        } else {
            SHA256_ZERO_HASHES[0]
        }
    }

    // --- Mutations ---

    pub fn append(&mut self, new_value: Hash) {
        let new_root = update_siblings_append_merkle_tree(&mut self.next_siblings, new_value, self.next_index);
        self.current_root = new_root;
        self.next_index += 1;
    }

    pub fn append_delta_merkle_proof(&mut self, new_value: Hash) -> DeltaMerkleProofCore<Hash> {
        let proof = self.get_partial_merkle_proof_for_current_index();
        let zero_leaf = SHA256_ZERO_HASHES[0];
        
        self.append(new_value);

        DeltaMerkleProofCore::from_params::<QSha256Hasher>(
            proof.index,
            zero_leaf,
            new_value,
            proof.siblings
        )
    }
    pub fn append_partial_delta_merkle_proof(&mut self, new_value: Hash) -> FixedMerkleAppendTreeDeltaPartialMerkleProof {
        let proof = self.get_partial_merkle_proof_fixed_for_current_index();
        let zero_leaf = SHA256_ZERO_HASHES[0];
        
        self.append(new_value);

        FixedMerkleAppendTreeDeltaPartialMerkleProof {
            new_value,
            old_value: zero_leaf,
            index: proof.index,
            siblings: proof.siblings,
        }
    }

    pub fn revert_to_next_index(&mut self, target_next_index: u64, changed_left_next_siblings: &[Hash]) -> QDogeResult<()> {
        if self.next_index == 0 || target_next_index >= self.next_index {
             return Err(DogeBridgeError::RevertIndexTooHigh);
        }
        if target_next_index == 0 {
            self.next_siblings = core::array::from_fn(|i| SHA256_ZERO_HASHES[i]);
            self.current_root = SHA256_ZERO_HASHES[TREE_HEIGHT];
            self.next_index = 0;
            return Ok(());
        }
        let changed_count = changed_left_next_siblings.len();
        let mut changed_idx = 0;
        let mut changed_index = target_next_index ^ self.next_index;
        for i in 0..32 {
            if changed_index == 0 {
                break;
            }
            let is_right_child = target_next_index & (1 << i) != 0;
            if is_right_child {
                // Right child: Sibling is stored Left.
                if changed_idx >= changed_count {
                    return Err(DogeBridgeError::NotEnoughChangedLeftSiblings);
                }
                self.next_siblings[i] = changed_left_next_siblings[changed_idx];
                changed_idx += 1;
            } else {
                // Left child: Sibling is Zero.
                self.next_siblings[i] = SHA256_ZERO_HASHES[i];
            }
            changed_index >>= 1;
        }

        if changed_idx != changed_count {
            //return Err(DogeBridgeError::TooManyChangedLeftSiblings);
        }

        let root = hash_impl_sha256_compute_merkle_root(&[0u8; 32], target_next_index, &self.next_siblings);
        self.current_root = root;
        self.next_index = target_next_index;
        Ok(())
        


    }
    /// Reverts the tree to `index`.
    /// `value` is the leaf at `index - 1`.
    /// `changed_left_siblings` are the left siblings required when the path from `index - 1` includes Right children.
    pub fn revert_to_index(&mut self, index: u64, changed_left_siblings: &[Hash], value: Hash) -> QDogeResult<()> {
        if self.next_index == 0 || index >= self.next_index {
             return Err(DogeBridgeError::RevertIndexTooHigh);
        }
        if index == 0 {
            *self = Self::new_empty();
            return Ok(());
        }

        // We reconstruct the state as it was when `next_index` was `index`.
        // The last added leaf was at `index - 1`.
        let mut current_hash = value;
        let mut revert_path_index = index - 1;
        let mut sibling_idx = 0;
        
        
        for i in 0..TREE_HEIGHT {
            let is_right_child = (revert_path_index & 1) == 1;

            if is_right_child {
                // The node at `index-1` is a Right child.
                // To compute the root, we need the Left child.
                if sibling_idx >= changed_left_siblings.len() {
                    return Err(DogeBridgeError::NotEnoughChangedLeftSiblings);
                }
                let left = changed_left_siblings[sibling_idx];
                sibling_idx += 1;

                current_hash = hash_impl_sha256_two_to_one_bytes(&left, &current_hash);
                
                // For the tree state at `index`, this level is now "fresh" (waiting for a Left child),
                // so `next_siblings[i]` is technically irrelevant/garbage until overwritten.
                // We leave it or zero it.
                self.next_siblings[i] = SHA256_ZERO_HASHES[i]; 
            } else {
                // The node at `index-1` is a Left child.
                // It is waiting for a Right child.
                // Therefore, it MUST be stored in `next_siblings`.
                self.next_siblings[i] = current_hash;
                
                // Compute temporary root
                current_hash = hash_impl_sha256_two_to_one_bytes(&current_hash, &SHA256_ZERO_HASHES[i]);
            }
            revert_path_index >>= 1;
        }

        if sibling_idx != changed_left_siblings.len() {
            return Err(DogeBridgeError::TooManyChangedLeftSiblings);
        }

        self.current_root = current_hash;
        self.next_index = index;

        Ok(())
    }

    // --- Proof Generation ---

    pub fn get_partial_merkle_proof_for_current_index(&self) -> MerkleProofCorePartial<Hash> {
        let index = self.next_index;
        // Proof for the empty slot at `next_index` (value is Zero)
        let value = SHA256_ZERO_HASHES[0];
        let mut siblings = Vec::with_capacity(TREE_HEIGHT);
        let mut temp_idx = index;

        for i in 0..TREE_HEIGHT {
            if (temp_idx & 1) == 1 {
                // Right child: Sibling is the stored Left
                siblings.push(self.next_siblings[i]);
            } else {
                // Left child: Sibling is Zero
                siblings.push(SHA256_ZERO_HASHES[i]);
            }
            temp_idx >>= 1;
        }

        MerkleProofCorePartial::new_from_params(index, value, siblings)
    }
    pub fn get_partial_merkle_proof_fixed_for_current_index(&self) -> FixedMerkleAppendTreePartialMerkleProof {
        let index = self.next_index;
        // Proof for the empty slot at `next_index` (value is Zero)
        let value = SHA256_ZERO_HASHES[0];
        let siblings = core::array::from_fn(|i| if (index >> i) & 1 == 1 {
            self.next_siblings[i]
        } else {
            SHA256_ZERO_HASHES[i]
        });
        FixedMerkleAppendTreePartialMerkleProof { index, value, siblings }
    }

    pub fn get_merkle_proof_for_current_index(&self) -> MerkleProofCore<Hash> {
        if self.next_index == 0 {
             return MerkleProofCore::new_from_params::<QSha256Hasher>(
                 0, 
                 SHA256_ZERO_HASHES[0], 
                 (0..TREE_HEIGHT).map(|i| SHA256_ZERO_HASHES[i]).collect()
             );
        }

        let index = self.next_index - 1;
        // We do not store the leaf value for index-1 (lossy tree), so we use Zero or best guess.
        // The proof structure is correct, but the value is just a placeholder.
        let value = SHA256_ZERO_HASHES[0]; 
        
        let mut siblings = Vec::with_capacity(TREE_HEIGHT);
        let mut temp_idx = index;

        for i in 0..TREE_HEIGHT {
            if (temp_idx & 1) == 1 {
                // We are Right. Sibling is stored Left.
                siblings.push(self.next_siblings[i]);
            } else {
                // We are Left. Sibling is Zero.
                siblings.push(SHA256_ZERO_HASHES[i]);
            }
            temp_idx >>= 1;
        }

        MerkleProofCore::new_from_params::<QSha256Hasher>(index, value, siblings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_consistency() {
        let mut tree = FixedMerkleAppendTree::new_empty();
        let leaf1 = [1u8; 32];
        let leaf2 = [2u8; 32];

        tree.append(leaf1);
        let root1 = tree.get_root();
        
        tree.append(leaf2);
        let root2 = tree.get_root();

        // Manual check root2: H(H(L1, L2), Zero...)
        let h1 = hash_impl_sha256_two_to_one_bytes(&leaf1, &leaf2);
        let mut expected = h1;
        for i in 1..TREE_HEIGHT {
            expected = hash_impl_sha256_two_to_one_bytes(&expected, &SHA256_ZERO_HASHES[i]);
        }
        assert_eq!(root2, expected);

        // Revert to 1
        // leaf1 was Left (index 0). No changed left siblings needed.
        tree.revert_to_index(1, &[], leaf1).unwrap();
        
        assert_eq!(tree.get_root(), root1);
        assert_eq!(tree.next_index, 1);
        assert_eq!(tree.next_siblings[0], leaf1);
    }
}