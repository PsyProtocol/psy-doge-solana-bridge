#![allow(dead_code)]
#![allow(unused_imports)]
use std::{collections::HashMap, marker::PhantomData};

use crate::crypto::hash::{merkle::{delta_merkle_proof::DeltaMerkleProofCore, merkle_proof::MerkleProofCore, utils::compute_root_merkle_proof_generic}, traits::MerkleZeroHasher};


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SimpleMerkleNodeKey {
    pub level: u8,
    pub index: u64,
}
impl SimpleMerkleNodeKey {
    pub fn new_root() -> Self {
        Self { level: 0, index: 0 }
    }
    pub fn new(level: u8, index: u64) -> Self {
        Self { level, index }
    }
    pub fn first_leaf_for_height(&self, height: u8) -> Self {
        if height <= self.level {
            self.clone()
        } else {
            let diff = (height - self.level) as u64;
            Self {
                level: height,
                index: (1u64 << diff) * self.index,
            }
        }
    }
    pub fn get_siblings_keys_to_height(&self, to_level: u8) -> Vec<SimpleMerkleNodeKey> {
        if to_level > self.level {
            vec![]
        } else {
            let mut my_node = self.clone();
            let mut siblings = Vec::with_capacity((self.level - to_level) as usize);
            while my_node.level != to_level {
                siblings.push(my_node.sibling());
                my_node = my_node.parent();
            }

            siblings
        }
    }
    pub fn sibling(&self) -> Self {
        Self {
            level: self.level,
            index: self.index ^ 1,
        }
    }
    pub fn siblings(&self) -> Vec<Self> {
        let mut result = Vec::with_capacity(self.level as usize);
        let mut current = *self;
        for _ in 0..self.level {
            result.push(current.sibling());
            current = current.parent();
        }
        result
    }

    // if self or other are on the same merkle path
    pub fn is_direct_path_related(&self, other: &SimpleMerkleNodeKey) -> bool {
        if other.level == self.level {
            self.index == other.index
        }else if other.level < self.level {
            // opt?: (self.index>>(self.level-other.level)) == other.index
            self.parent_at_level(other.level).index == other.index

        }else{
            other.parent_at_level(self.level).index == self.index
        }
    }
    pub fn parent(&self) -> Self {
        if self.level == 0 {
            return *self;
        }
        Self {
            level: self.level - 1,
            index: self.index >> 1,
        }
    }
    pub fn first_leaf_child(&self, tree_height: u8) -> Self {
        Self {
            level: tree_height,
            index: self.index << (tree_height - self.level),
        }
    }
    pub fn left_child(&self) -> Self {
        Self {
            level: self.level + 1,
            index: self.index << 1,
        }
    }
    pub fn right_child(&self) -> Self {
        Self {
            level: self.level + 1,
            index: (self.index << 1) + 1,
        }
    }
    pub fn is_on_the_right_of(&self, other: &SimpleMerkleNodeKey) -> bool {
        if other.level == self.level {
            self.index > other.index
        } else if other.level < self.level {
            self.parent_at_level(other.level).index > other.index
        } else {
            self.index > other.parent_at_level(self.level).index
        }
    }
    pub fn is_to_the_left_of(&self, other: &SimpleMerkleNodeKey) -> bool {
        if other.level == self.level {
            self.index < other.index
        } else if other.level < self.level {
            self.parent_at_level(other.level).index < other.index
        } else {
            self.index < other.parent_at_level(self.level).index
        }
    }

    pub fn parent_at_level(&self, level: u8) -> Self {
        if level > self.level {
            panic!("given level is not above this node")
        }
        self.n_th_ancestor(self.level - level)
    }
    pub fn n_th_ancestor(&self, levels_above: u8) -> Self {
        if levels_above >= self.level {
            Self::new_root()
        } else {
            Self {
                level: self.level - levels_above,
                index: self.index >> levels_above,
            }
        }
    }
    pub fn find_nearest_common_ancestor(&self, other: &SimpleMerkleNodeKey) -> SimpleMerkleNodeKey {
        let start_level = u8::min(other.level, self.level);
        let mut self_current = self.parent_at_level(start_level);
        let mut other_current = other.parent_at_level(start_level);
        while !other_current.eq(&self_current) {
            self_current = self_current.parent();
            other_current = other_current.parent();
        }
        self_current
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SimpleMerkleNode<Hash> {
    pub key: SimpleMerkleNodeKey,
    pub value: Hash,
}

impl<Hash> SimpleMerkleNode<Hash> {
    pub fn new_root(value: Hash) -> Self {
        Self {
            key: SimpleMerkleNodeKey::new_root(),
            value,
        }
    }
    pub fn new(level: u8, index: u64, value: Hash) -> Self {
        Self {
            key: SimpleMerkleNodeKey { level, index },
            value,
        }
    }
}
#[derive(Debug, Clone)]
pub struct SimpleMemoryMerkleRecorderStore<Hasher, Hash: Copy + PartialEq + Default> {
    nodes: HashMap<SimpleMerkleNodeKey, Hash>,
    updated_nodes: HashMap<SimpleMerkleNodeKey, Hash>,
    height: u8,
    effective_height: u8,
    _hasher: PhantomData<Hasher>,
}

impl<Hasher: MerkleZeroHasher<Hash>, Hash: Copy + PartialEq + Default>
    SimpleMemoryMerkleRecorderStore<Hasher, Hash>
{
    pub fn new(height: u8) -> Self {
        Self {
            nodes: HashMap::new(),
            updated_nodes: HashMap::new(),
            height,
            effective_height: height,
            _hasher: PhantomData::default(),
        }
    }
    pub fn set_effective_height(&mut self, effective_height: u8) {
        self.effective_height = effective_height;
    }
    pub fn get_old_root(&self) -> Hash {
        self.get_last_commit_root()
    }
    pub fn injest_commit_delta_merkle_proof_old_nodes_new_update(&mut self, dmp: &DeltaMerkleProofCore<Hash>) -> anyhow::Result<()> {

        let base_key = SimpleMerkleNodeKey::new(self.height, dmp.index);

        if self.get_height() as usize != dmp.siblings.len() {
            anyhow::bail!("proof height does not match tree height");
        }

        let mut current_key = base_key;
        let mut current_old_hash = dmp.old_value;
        let mut current_new_hash = dmp.new_value;
            self.nodes.insert(current_key, current_old_hash);
            self.updated_nodes.insert(base_key, current_new_hash);

        for sibling_hash in &dmp.siblings {

            let sibling_key = current_key.sibling();
            self.nodes.insert(sibling_key, *sibling_hash);
            current_old_hash = if (current_key.index & 1) == 1 {
                Hasher::two_to_one(sibling_hash, &current_old_hash)
            } else {
                Hasher::two_to_one(&current_old_hash, sibling_hash)
            };

            current_new_hash = if (current_key.index & 1) == 1 {
                Hasher::two_to_one(sibling_hash, &current_new_hash)
            } else {
                Hasher::two_to_one(&current_new_hash, sibling_hash)
            };

            self.nodes.insert(current_key, current_old_hash);
            self.updated_nodes.insert(base_key, current_new_hash);
            current_key = current_key.parent();
        }
        Ok(())
    }
    pub fn from_hash_map(
        height: u8,
        nodes: HashMap<SimpleMerkleNodeKey, Hash>,
    ) -> Self {
        Self {
            nodes,
            updated_nodes: HashMap::new(),
            height,
            effective_height: height,
            _hasher: PhantomData::default(),
        }
    }
    pub fn is_empty_root(&self) -> bool {
        self.get_root() == Hasher::get_zero_hash(self.height as usize)
    }
    pub fn get_changes(&self) -> &HashMap<SimpleMerkleNodeKey, Hash> {
        &self.updated_nodes
    }
    pub fn get_changes_merkle_nodes(&self) -> Vec<SimpleMerkleNode<Hash>> {
        self.updated_nodes.iter().map(|(k, v)| SimpleMerkleNode{
            key: *k,
            value: *v,
        }).collect()
    }

    pub fn injest_merkle_proof(&mut self, proof: &MerkleProofCore<Hash>) -> anyhow::Result<()> {
        let base_key = SimpleMerkleNodeKey::new(proof.siblings.len() as u8, proof.index);
        let mut current_key = base_key;
        
        if self.get_height() as usize != proof.siblings.len() {
            anyhow::bail!("proof height does not match tree height");
        }
        self.set_node_value(current_key, proof.value);

        for sibling_hash in &proof.siblings {
            let sibling_key = current_key.sibling();
            self.set_node_value(sibling_key, *sibling_hash);
            current_key = current_key.parent();
        }
        self.rehash_from_node_to_level(base_key, 0);
        Ok(())
    }

    pub fn injest_merkle_proof_into_nodes(&mut self, proof: &MerkleProofCore<Hash>) -> anyhow::Result<()> {
        let base_key = SimpleMerkleNodeKey::new(proof.siblings.len() as u8, proof.index);
        let mut current_key = base_key;
        
        if self.get_height() as usize != proof.siblings.len() {
            anyhow::bail!("proof height does not match tree height");
        }
        self.set_node_value_nodes_no_updated(current_key, proof.value);

        for sibling_hash in &proof.siblings {
            let sibling_key = current_key.sibling();
            self.set_node_value_nodes_no_updated(sibling_key, *sibling_hash);
            current_key = current_key.parent();
        }
        self.rehash_from_node_to_level_no_updated(base_key, 0);
        Ok(())
    }

    pub fn injest_merkle_proof_(&mut self, proof: &MerkleProofCore<Hash>) -> anyhow::Result<()> {
        let base_key = SimpleMerkleNodeKey::new(proof.siblings.len() as u8, proof.index);
        let mut current_key = base_key;
        
        if self.get_height() as usize != proof.siblings.len() {
            anyhow::bail!("proof height does not match tree height");
        }
        self.set_node_value(current_key, proof.value);

        for sibling_hash in &proof.siblings {
            let sibling_key = current_key.sibling();
            self.set_node_value(sibling_key, *sibling_hash);
            current_key = current_key.parent();
        }
        self.rehash_from_node_to_level(base_key, 0);
        Ok(())
    }
    pub fn commit_changes(&mut self) {
        for entry in self.updated_nodes.iter() {
            self.nodes.insert(*entry.0, *entry.1);
        }
        self.updated_nodes.clear();
    }
    pub fn revert_changes(&mut self) {
        self.updated_nodes.clear();
    }
    pub fn get_height(&self) -> u8 {
        self.height
    }
    pub fn get_max_leaf_index(&self) -> u64 {
        (1u64 << (self.height as u64)) - 1u64
    }
    pub fn set_node_value(&mut self, key: SimpleMerkleNodeKey, value: Hash) {
        self.updated_nodes.insert(key, value);
    }
    pub fn set_node_value_nodes_no_updated(&mut self, key: SimpleMerkleNodeKey, value: Hash) {
        self.nodes.insert(key, value);
    }
    pub fn clear_changes_remove_committed_leaves_and_rehash(&mut self, start_leaf_index: u64, end_leaf_index: u64) {
        self.updated_nodes.clear();
        for i in start_leaf_index..end_leaf_index {
            self.nodes.remove(&SimpleMerkleNodeKey{
                level: self.height,
                index: i,
            });
        }
        self.rehash_range_committed(self.height, start_leaf_index, end_leaf_index+1);

    }
    pub fn get_node_value(&self, key: &SimpleMerkleNodeKey) -> Hash {
        if self.updated_nodes.contains_key(key){
            self.updated_nodes[key]
        }else if self.nodes.contains_key(key) {
            self.nodes[key]
        } else {
            assert!(
                self.height >= key.level,
                "requested node value of invalid key level for this tree"
            );
            Hasher::get_zero_hash((self.height - key.level) as usize)
        }
    }
    pub fn get_node_value_no_updated(&self, key: &SimpleMerkleNodeKey) -> Hash {
        if self.nodes.contains_key(key) {
            self.nodes[key]
        } else {
            assert!(
                self.height >= key.level,
                "requested node value of invalid key level for this tree"
            );
            Hasher::get_zero_hash((self.height - key.level) as usize)
        }
    }
    pub fn get_last_commit_node(&self, key: &SimpleMerkleNodeKey) -> Hash {
        if self.nodes.contains_key(key) {
            self.nodes[key]
        } else {
            Hasher::get_zero_hash((self.height - key.level) as usize)
        }
    }
    pub fn get_last_commit_root(&self) -> Hash {
        self.get_last_commit_node(&SimpleMerkleNodeKey::new_root())
    }

    pub fn get_root(&self) -> Hash {
        self.get_node_value(&SimpleMerkleNodeKey::new_root())
    }

    pub fn get_e_leaf_value(&self, index: u64) -> Hash {
        self.get_node_value(&SimpleMerkleNodeKey::new(self.effective_height, index))
    }
    pub fn get_e_leaf(&self, index: u64) -> MerkleProofCore<Hash> {
        let leaf_key = SimpleMerkleNodeKey::new(self.effective_height, index);
        let value = self.get_e_leaf_value(index);

        let mut current_sibling = leaf_key.sibling();
        let mut siblings = Vec::with_capacity(self.effective_height as usize);

        while current_sibling.level > 0 {
            siblings.push(self.get_node_value(&current_sibling));
            current_sibling = current_sibling.parent().sibling();
        }

        let root = self.get_root();

        MerkleProofCore {
            index,
            siblings,
            root,
            value,
        }
    }

    pub fn get_leaf_value(&self, index: u64) -> Hash {
        self.get_node_value(&SimpleMerkleNodeKey::new(self.height, index))
    }

    pub fn get_leaf(&self, index: u64) -> MerkleProofCore<Hash> {
        let leaf_key = SimpleMerkleNodeKey::new(self.height, index);
        let value = self.get_leaf_value(index);

        let mut current_sibling = leaf_key.sibling();
        let mut siblings = Vec::with_capacity(self.height as usize);

        while current_sibling.level > 0 {
            siblings.push(self.get_node_value(&current_sibling));
            current_sibling = current_sibling.parent().sibling();
        }

        let root = self.get_root();

        MerkleProofCore {
            index,
            siblings,
            root,
            value,
        }
    }
    pub fn get_historical_merkle_proof(&self, index: u64) -> MerkleProofCore<Hash> {
        let leaf_key = SimpleMerkleNodeKey::new(self.height, index);
        let value = self.get_leaf_value(index);

        let mut current_sibling = leaf_key.sibling();
        let mut siblings = Vec::with_capacity(self.height as usize);

        while current_sibling.level > 0 {
            let value = if current_sibling.index & 1 == 1 {
                Hasher::get_zero_hash(self.height as usize - current_sibling.level as usize)
            } else {
                self.get_node_value(&current_sibling)
            };
            siblings.push(value);
            current_sibling = current_sibling.parent().sibling();
        }

        let root = compute_root_merkle_proof_generic::<Hash, Hasher>(value, index, &siblings);

        MerkleProofCore {
            index,
            siblings,
            root,
            value,
        }
    }

    pub fn get_historical_pivot_leaf(&self, index: u64) -> MerkleProofCore<Hash> {
        let leaf_key = SimpleMerkleNodeKey::new(self.height, index);
        let value = Hasher::get_zero_hash(0);

        let mut current_sibling = leaf_key.sibling();
        let mut siblings = Vec::with_capacity(self.height as usize);

        while current_sibling.level > 0 {
            let value = if current_sibling.index & 1 == 1 {
                Hasher::get_zero_hash(self.height as usize - current_sibling.level as usize)
            } else {
                self.get_node_value(&current_sibling)
            };
            siblings.push(value);
            current_sibling = current_sibling.parent().sibling();
        }

        let root = compute_root_merkle_proof_generic::<Hash, Hasher>(value, index, &siblings);
        

        MerkleProofCore {
            index,
            siblings,
            root,
            value,
        }
    }
    pub fn find_first_non_zero_leaf(&self, node: SimpleMerkleNodeKey) -> Option<u64> {
        let value = self.get_node_value(&node);
        let zero_hash = Hasher::get_zero_hash((self.height - node.level) as usize);
        if value.eq(&zero_hash) {
            Some(node.first_leaf_child(self.height).index)
        } else {
            if self.height == node.level {
                if value.eq(&zero_hash) {
                    Some(node.index)
                } else {
                    None
                }
            } else {
                match self.find_first_non_zero_leaf(node.left_child()) {
                    Some(ind) => Some(ind),
                    None => self.find_first_non_zero_leaf(node.right_child()),
                }
            }
        }
    }
    pub fn find_next_append_index(&self) -> anyhow::Result<u64> {
        if self
            .get_root()
            .eq(&Hasher::get_zero_hash(self.height as usize))
        {
            return Ok(0);
        } else if self.height == 0 {
            anyhow::bail!("tree is full");
        } else {
            /*
            let mut cur_node = SimpleMerkleNodeKey::new_root();
            while cur_node.level < self.height {
                let child_zero_hash =
                    Hasher::get_zero_hash((self.height - (cur_node.level + 1)) as usize);
                let left_child = cur_node.left_child();
                let right_child = cur_node.right_child();
                println!("cur_node: {:?}", cur_node);
                if self.get_node_value(&left_child).eq(&child_zero_hash) {
                    return Ok(left_child.first_leaf_child(self.height).index);
                } else if self.get_node_value(&right_child).eq(&child_zero_hash) {
                    cur_node = left_child;
                } else {
                    cur_node = right_child;
                }
            }

            anyhow::bail!("tree is full");*/

            match self.find_first_non_zero_leaf(SimpleMerkleNodeKey::new_root()) {
                Some(ind) => Ok(ind),
                None => anyhow::bail!("tree is full"),
            }
        }
    }
    pub fn rehash_from_node_to_level(&mut self, node: SimpleMerkleNodeKey, root_level: u8) {
        let mut current = node;
        let mut current_value = self.get_node_value(&current);
        while current.level > root_level {
            let parent_key = current.parent();
            let sibling_value = self.get_node_value(&current.sibling());

            let parent_value = if (current.index & 1) == 1 {
                Hasher::two_to_one(&sibling_value, &current_value)
            } else {
                Hasher::two_to_one(&current_value, &sibling_value)
            };
            self.set_node_value(parent_key, parent_value);
            current = parent_key;
            current_value = parent_value;
        }
    }
    pub fn rehash_from_node_to_level_no_updated(&mut self, node: SimpleMerkleNodeKey, root_level: u8) {
        let mut current = node;
        let mut current_value = self.get_node_value_no_updated(&current);
        while current.level > root_level {
            let parent_key = current.parent();
            let sibling_value = self.get_node_value_no_updated(&current.sibling());

            let parent_value = if (current.index & 1) == 1 {
                Hasher::two_to_one(&sibling_value, &current_value)
            } else {
                Hasher::two_to_one(&current_value, &sibling_value)
            };
            self.set_node_value_nodes_no_updated(parent_key, parent_value);
            current = parent_key;
            current_value = parent_value;
        }
    }

    pub fn rehash_sub_tree_dmp(
        &mut self,
        sub_tree_height: u8,
        sub_tree_index: u64,
    ) -> DeltaMerkleProofCore<Hash> {
        let sub_tree_root_level = self.height - sub_tree_height;
        let sub_root_node = SimpleMerkleNodeKey::new(sub_tree_root_level, sub_tree_index);
        let old_sub_tree_root = self.get_node_value(&sub_root_node);
        let old_tree_root = self.get_root();

        self.rehash_sub_tree(sub_tree_height, sub_tree_index);

        let new_sub_tree_root = self.get_node_value(&sub_root_node);
        let new_tree_root = self.get_root();

        let siblings = sub_root_node
            .siblings()
            .iter()
            .map(|x| self.get_node_value(x))
            .collect::<Vec<_>>();

        DeltaMerkleProofCore {
            old_root: old_tree_root,
            old_value: old_sub_tree_root,
            new_root: new_tree_root,
            new_value: new_sub_tree_root,
            index: sub_tree_index,
            siblings,
        }
    }
    pub fn rehash_sub_tree(&mut self, sub_tree_height: u8, sub_tree_index: u64) -> Hash {
        if sub_tree_height == 0 {
            return self.get_leaf_value(sub_tree_index);
        } else if sub_tree_height == 1 {
            let left_key = SimpleMerkleNodeKey::new(self.height, sub_tree_index * 2);
            let v = Hasher::two_to_one(
                &self.get_node_value(&left_key),
                &self.get_node_value(&SimpleMerkleNodeKey::new(self.height, sub_tree_index * 2)),
            );
            self.set_node_value(left_key.parent(), v);
            return v;
        }

        let sub_tree_root_level = self.height - sub_tree_height;

        let mut child_base_key = SimpleMerkleNodeKey::new(
            self.height,
            (sub_tree_index) * (1u64 << (sub_tree_height as u64)),
        );

        let mut nodes_at_current_level = 1usize << (sub_tree_height - 1);

        let mut child_values = Vec::with_capacity(nodes_at_current_level);

        for i in 0..(nodes_at_current_level as u64) {
            let left_key =
                SimpleMerkleNodeKey::new(child_base_key.level, i * 2 + child_base_key.index);

            let v = Hasher::two_to_one(
                &self.get_node_value(&left_key),
                &self.get_node_value(&left_key.sibling()),
            );

            self.set_node_value(left_key.parent(), v);
            child_values.push(v);
        }
        nodes_at_current_level = nodes_at_current_level >> 1;
        child_base_key = child_base_key.parent();

        while child_base_key.level > sub_tree_root_level {
            let mut parent_values = Vec::with_capacity(nodes_at_current_level as usize);
            for i in 0..nodes_at_current_level {
                let parent_key = SimpleMerkleNodeKey::new(
                    child_base_key.level - 1,
                    i as u64 + (child_base_key.index >> 1u64),
                );
                let parent_value =
                    Hasher::two_to_one(&child_values[i * 2], &child_values[i * 2 + 1]);

                self.set_node_value(parent_key, parent_value);
                parent_values.push(parent_value);
            }
            nodes_at_current_level = nodes_at_current_level >> 1;
            child_base_key = child_base_key.parent();
            child_values = parent_values;
        }

        self.rehash_from_node_to_level(child_base_key, 0);

        child_values[0]
    }
pub fn rehash_range(
        &mut self,
        node_base_level: u8,
        start_node_index_inclusive: u64,
        end_node_index_exclusive: u64,
    ) {
        if start_node_index_inclusive >= end_node_index_exclusive {
            return;
        }

        let mut current_level = node_base_level;
        let mut start = start_node_index_inclusive;
        let mut end = end_node_index_exclusive;

        // Iterate upwards from the base level to the root (level 0)
        while current_level > 0 {
            let parent_level = current_level - 1;
            
            // Calculate the range of parent indices that cover the current range.
            // Parent index i covers children indices 2*i and 2*i + 1.
            // The range starts at floor(start / 2).
            let parent_start = start >> 1;
            // The range ends at floor((end - 1) / 2) + 1.
            // We use (end-1) because end is exclusive.
            let parent_end = ((end - 1) >> 1) + 1;

            for parent_index in parent_start..parent_end {
                let left_child_idx = parent_index << 1;
                let right_child_idx = left_child_idx + 1;

                let left_key = SimpleMerkleNodeKey::new(current_level, left_child_idx);
                let right_key = SimpleMerkleNodeKey::new(current_level, right_child_idx);

                // We use get_node_value here. 
                // 1. If the node was just updated in the previous batch step, it's in updated_nodes.
                // 2. If it's a sibling outside the batch range, it's retrieved from storage or calculated as zero hash.
                let left_val = self.get_node_value(&left_key);
                let right_val = self.get_node_value(&right_key);

                let parent_val = Hasher::two_to_one(&left_val, &right_val);
                
                self.set_node_value(
                    SimpleMerkleNodeKey::new(parent_level, parent_index),
                    parent_val,
                );
            }

            // Move coordinates up one level
            start = parent_start;
            end = parent_end;
            current_level -= 1;
        }
    }

    fn rehash_range_committed(
        &mut self,
        node_base_level: u8,
        start_node_index_inclusive: u64,
        end_node_index_exclusive: u64,
    ) {
        if start_node_index_inclusive >= end_node_index_exclusive {
            return;
        }

        let mut current_level = node_base_level;
        let mut start = start_node_index_inclusive;
        let mut end = end_node_index_exclusive;

        // Iterate upwards from the base level to the root (level 0)
        while current_level > 0 {
            let parent_level = current_level - 1;
            
            // Calculate the range of parent indices that cover the current range.
            // Parent index i covers children indices 2*i and 2*i + 1.
            // The range starts at floor(start / 2).
            let parent_start = start >> 1;
            // The range ends at floor((end - 1) / 2) + 1.
            // We use (end-1) because end is exclusive.
            let parent_end = ((end - 1) >> 1) + 1;

            for parent_index in parent_start..parent_end {
                let left_child_idx = parent_index << 1;
                let right_child_idx = left_child_idx + 1;

                let left_key = SimpleMerkleNodeKey::new(current_level, left_child_idx);
                let right_key = SimpleMerkleNodeKey::new(current_level, right_child_idx);

                // We use get_node_value here. 
                // 1. If the node was just updated in the previous batch step, it's in updated_nodes.
                // 2. If it's a sibling outside the batch range, it's retrieved from storage or calculated as zero hash.
                let left_val = self.get_node_value(&left_key);
                let right_val = self.get_node_value(&right_key);

                let parent_val = Hasher::two_to_one(&left_val, &right_val);
                
                self.nodes.insert(
                    SimpleMerkleNodeKey::new(parent_level, parent_index),
                    parent_val,
                );
            }

            // Move coordinates up one level
            start = parent_start;
            end = parent_end;
            current_level -= 1;
        }
    }

    pub fn fast_batch_set_leaves(
        &mut self,
        start_index: u64,
        values: &[Hash],
    ) {
        for (i, v) in values.iter().enumerate() {
            self.set_node_value(
                SimpleMerkleNodeKey::new(self.height, start_index + i as u64),
                *v,
            );
        }
        self.rehash_range(
            self.height,
            start_index,
            start_index + (values.len() as u64)
        );
    }

    pub fn update_sub_tree(
        &mut self,
        sub_tree_height: u8,
        sub_tree_index: u64,
        sub_tree_offset_index: u64,
        values: &[Hash],
    ) -> anyhow::Result<Hash> {
        let leaves_per_sub_tree = 1u64 << (sub_tree_height as u64);
        if (sub_tree_offset_index + (values.len() as u64)) >= leaves_per_sub_tree {
            anyhow::bail!("cannot set more values in a sub tree than it can contain");
        }
        let offset_index = leaves_per_sub_tree * sub_tree_index + sub_tree_offset_index;
        for (i, v) in values.iter().enumerate() {
            self.set_node_value(
                SimpleMerkleNodeKey::new(self.height, offset_index + i as u64),
                *v,
            );
        }
        Ok(self.rehash_sub_tree(sub_tree_height, sub_tree_index))
    }

    fn _set_sub_tree(
        &mut self,
        sub_tree_height: u8,
        sub_tree_index: u64,
        leaves: &[Hash],
    ) -> anyhow::Result<()> {
        if leaves.len() == 0 {}

        if leaves.len() > (1usize << (sub_tree_height)) {
            anyhow::bail!("cannot set more leaves than can fit in a subtree");
        }

        if sub_tree_height == 0 {
            self.set_leaf(sub_tree_index, leaves[0]);
            return Ok(());
        }

        let offset_index = (1u64 << sub_tree_height) * sub_tree_index;
        for (i, v) in leaves.iter().enumerate() {
            self.set_node_value(
                SimpleMerkleNodeKey::new(self.height, offset_index + i as u64),
                *v,
            );
        }
        self.rehash_sub_tree(sub_tree_height, sub_tree_index);

        Ok(())
    }
    fn _set_sub_tree_dmp(
        &mut self,
        sub_tree_height: u8,
        sub_tree_index: u64,
        leaves: &[Hash],
    ) -> anyhow::Result<DeltaMerkleProofCore<Hash>> {
        if leaves.len() == 0 {
            anyhow::bail!("cannot set a sub tree of 0 length");
        }

        if leaves.len() > (1usize << (sub_tree_height)) {
            anyhow::bail!("cannot set more leaves than can fit in a subtree");
        }

        if sub_tree_height == 0 {
            return Ok(self.set_leaf(sub_tree_index, leaves[0]));
        }

        let offset_index = (1u64 << sub_tree_height) * sub_tree_index;
        for (i, v) in leaves.iter().enumerate() {
            self.set_node_value(
                SimpleMerkleNodeKey::new(self.height, offset_index + i as u64),
                *v,
            );
        }
        
        Ok(self.rehash_sub_tree_dmp(sub_tree_height, sub_tree_index))
    }

    pub fn set_e_leaf(&mut self, index: u64, value: Hash) -> DeltaMerkleProofCore<Hash> {
        let old_proof = self.get_e_leaf(index);
        let mut current_value = value;
        let mut current_key = SimpleMerkleNodeKey::new(self.effective_height, index);

        let height = self.effective_height as usize;
        for i in 0..height {
            let new_key = current_key.parent();
            let index = current_key.index;
            self.set_node_value(current_key, current_value);

            current_value = if index & 1 == 0 {
                Hasher::two_to_one(&current_value, &old_proof.siblings[i])
            } else {
                Hasher::two_to_one(&old_proof.siblings[i], &current_value)
            };
            current_key = new_key;
        }
        self.set_node_value(current_key, current_value);
        DeltaMerkleProofCore {
            old_root: old_proof.root,
            old_value: old_proof.value,

            new_root: current_value,
            new_value: value,

            siblings: old_proof.siblings,
            index: index,
        }
    }
    pub fn set_leaf(&mut self, index: u64, value: Hash) -> DeltaMerkleProofCore<Hash> {
        let old_proof = self.get_leaf(index);
        let mut current_value = value;
        let mut current_key = SimpleMerkleNodeKey::new(self.height, index);

        let height = self.height as usize;
        for i in 0..height {
            let new_key = current_key.parent();
            let index = current_key.index;
            self.set_node_value(current_key, current_value);

            current_value = if index & 1 == 0 {
                Hasher::two_to_one(&current_value, &old_proof.siblings[i])
            } else {
                Hasher::two_to_one(&old_proof.siblings[i], &current_value)
            };
            current_key = new_key;
        }
        self.set_node_value(current_key, current_value);
        DeltaMerkleProofCore {
            old_root: old_proof.root,
            old_value: old_proof.value,

            new_root: current_value,
            new_value: value,

            siblings: old_proof.siblings,
            index: index,
        }
    }
    pub fn set_e_leaf_no_proof(&mut self, index: u64, value: Hash) -> Hash {
        let old_proof = self.get_e_leaf(index);
        let mut current_value = value;
        let mut current_key = SimpleMerkleNodeKey::new(self.effective_height, index);

        let height = self.effective_height as usize;
        for i in 0..height {
            let new_key = current_key.parent();
            let index = current_key.index;
            self.set_node_value(current_key, current_value);

            current_value = if index & 1 == 0 {
                Hasher::two_to_one(&current_value, &old_proof.siblings[i])
            } else {
                Hasher::two_to_one(&old_proof.siblings[i], &current_value)
            };
            current_key = new_key;
        }
        self.set_node_value(current_key, current_value);
        current_value

    }
    pub fn set_leaf_no_proof(&mut self, index: u64, value: Hash) -> Hash {
        let old_proof = self.get_leaf(index);
        let mut current_value = value;
        let mut current_key = SimpleMerkleNodeKey::new(self.height, index);

        let height = self.height as usize;
        for i in 0..height {
            let new_key = current_key.parent();
            let index = current_key.index;
            self.set_node_value(current_key, current_value);

            current_value = if index & 1 == 0 {
                Hasher::two_to_one(&current_value, &old_proof.siblings[i])
            } else {
                Hasher::two_to_one(&old_proof.siblings[i], &current_value)
            };
            current_key = new_key;
        }
        self.set_node_value(current_key, current_value);
        current_value

    }
    pub fn get_siblings_for_key(
        &self,
        key: SimpleMerkleNodeKey,
    ) -> Vec<Hash> {
        let mut siblings = Vec::with_capacity(key.level as usize);
        let sibling_keys = key.get_siblings_keys_to_height(0);
        for current_sibling in sibling_keys {
            siblings.push(self.get_node_value(&current_sibling));
        }
        siblings
    }
    pub fn get_siblings_for_leaf(
        &self,
        leaf_index: u64,
    ) -> Vec<Hash> {
        let leaf_key = SimpleMerkleNodeKey::new(self.height, leaf_index);
        self.get_siblings_for_key(leaf_key)
    }
    pub fn get_subtree_merkle_proof(
        &self,
        root_level: u8,
        subtree_leaf_node: SimpleMerkleNodeKey,
    ) -> MerkleProofCore<Hash> {
        if root_level > subtree_leaf_node.level {
            panic!("root_level > leaf node level");
        }
        let level_difference = subtree_leaf_node.level - root_level;

        let leaf_key = subtree_leaf_node;
        let value = self.get_node_value(&leaf_key);
        if level_difference == 0 {
            return MerkleProofCore {
                root: value,
                value: value,
                siblings: Vec::new(),
                index: subtree_leaf_node.index,
            };
        }

        let mut current_sibling = leaf_key.sibling();
        let mut siblings = Vec::with_capacity(level_difference as usize);

        while current_sibling.level > root_level {
            siblings.push(self.get_node_value(&current_sibling));
            current_sibling = current_sibling.parent().sibling();
        }

        let root = self.get_node_value(&subtree_leaf_node.parent_at_level(root_level));

        MerkleProofCore {
            index: subtree_leaf_node.index,
            siblings,
            root,
            value,
        }
    }


    pub fn get_leaf_in_subtree(
        &self,
        root_level: u8,
        leaf_level: u8,
        leaf_index: u64,
    ) -> MerkleProofCore<Hash> {
        self.get_subtree_merkle_proof(root_level, SimpleMerkleNodeKey::new(leaf_level, leaf_index))
    }

    pub fn gen_fast_tree_inclusion_proofs(
        height: u8,
        leaves: &[Hash],
    ) -> anyhow::Result<Vec<MerkleProofCore<Hash>>> {
        let max_leaves = (1u64 << (height as u64)) as usize;
        let leaves_count = leaves.len();
        if leaves_count > max_leaves {
            anyhow::bail!("too many leaves for a tree of height {} (tried to add {} leaves, but max is {} leaves for this height)", height, leaves_count, max_leaves);
        } else {
            let mut tmp_tree = Self::new(height);
            for i in 0..leaves_count {
                tmp_tree.set_leaf(i as u64, leaves[i]);
            }

            let inclusion_proofs = (0..leaves_count)
                .map(|i| tmp_tree.get_leaf(i as u64))
                .collect::<Vec<_>>();

            Ok(inclusion_proofs)
        }
    }
}

pub fn get_merkle_proofs_for_compact<Hasher: MerkleZeroHasher<Hash>, Hash: Copy + PartialEq + Default>(from_index: u64, siblings: &[Hash], values: &[Hash]) -> Vec<MerkleProofCore<Hash>> {
    let mut tree = SimpleMemoryMerkleRecorderStore::<Hasher, Hash>::new(siblings.len() as u8);
    let key = SimpleMerkleNodeKey{
        index: from_index,
        level: siblings.len() as u8,
    };
    let mut sibling_key = key.sibling();
    for s in siblings.iter() {
        tree.set_node_value(sibling_key, *s);
        sibling_key = sibling_key.parent().sibling();
    }
    for i in 0..values.len() {
        tree.set_leaf(from_index + i as u64, values[i]);
    }

    (0..values.len()).map(|i| tree.get_leaf(i as u64 + from_index)).collect()



}

