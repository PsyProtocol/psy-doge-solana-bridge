/*
Copyright (C) 2025 Zero Knowledge Labs Limited, Psy Protocol
... (License and Attribution preserved)
*/

use crate::{
    common_types::QHash256, crypto::hash::{merkle::{delta_merkle_proof::DeltaMerkleProofCore, merkle_proof::{MerkleProofCore, MerkleProofCorePartial}}, sha256::{QSha256Hasher, SHA256_ZERO_HASHES}, sha256_impl::{hash_impl_sha256_compute_merkle_root, hash_impl_sha256_two_to_one_bytes}}, error::{DogeBridgeError, QDogeResult}
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

    pub fn new_empty_from_zero_hashes(
        next_index: u64,
        _zero_hashes: [Hash; TREE_HEIGHT],
    ) -> Self {
        if next_index == 0 {
             Self::new_empty()
        } else {
             panic!("Cannot create empty tree with non-zero index without siblings");
        }
    }

    pub fn new_vec(
        next_index: u64,
        zero_hashes: Vec<Hash>,
        siblings: Vec<Hash>,
        value: Hash,
    ) -> Self {
        // We assert zero_hashes length but ignore content as we use global SHA256 constants
        assert_eq!(zero_hashes.len(), TREE_HEIGHT);
        assert_eq!(siblings.len(), TREE_HEIGHT);
        
        let siblings_arr: [Hash; TREE_HEIGHT] = match siblings.try_into() {
            Ok(x) => x,
            Err(_) => panic!("Invalid siblings length"),
        };
        
        Self::new(next_index, siblings_arr, value)
    }

    pub fn new(
        next_index: u64,
        siblings: [Hash; TREE_HEIGHT],
        value: Hash,
    ) -> Self {
        if next_index == 0 {
            return Self::new_empty();
        }

        let mut next_siblings = core::array::from_fn(|i| SHA256_ZERO_HASHES[i]);
        let mut current = value;
        let mut current_index = next_index - 1;

        for i in 0..TREE_HEIGHT {
            let sibling = siblings[i];
            let is_right_child = (current_index & 1) == 1;

            if is_right_child {
                // We are Right. The provided sibling is Left.
                // We consume it to compute root, but we don't store it in frontier
                // because the next append at this level will be a fresh Left child.
                current = hash_impl_sha256_two_to_one_bytes(&sibling, &current);
            } else {
                // We are Left. We store ourselves in frontier for the future Right.
                next_siblings[i] = current;
                // We hash with Zero to compute the temporary root.
                current = hash_impl_sha256_two_to_one_bytes(&current, &SHA256_ZERO_HASHES[i]);
            }
            
            current_index >>= 1;
        }

        Self {
            next_index,
            current_root: current,
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
        let mut current = new_value;
        let mut index = self.next_index;

        for i in 0..TREE_HEIGHT {
            if (index & 1) == 0 {
                // Left Child
                self.next_siblings[i] = current;
                current = hash_impl_sha256_two_to_one_bytes(&current, &SHA256_ZERO_HASHES[i]);
            } else {
                // Right Child
                let left = self.next_siblings[i];
                current = hash_impl_sha256_two_to_one_bytes(&left, &current);
            }
            index >>= 1;
        }

        self.current_root = current;
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