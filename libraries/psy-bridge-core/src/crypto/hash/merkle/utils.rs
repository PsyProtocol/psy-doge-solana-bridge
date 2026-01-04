
use crate::crypto::hash::traits::MerkleHasher;


pub fn compute_root_merkle_proof_generic<Hash: PartialEq + Copy, H: MerkleHasher<Hash>>(
    value: Hash,
    index: u64,
    siblings: &[Hash]
) -> Hash {
    let mut current = value;
    let mut ind_tracker = index;
    for sibling in siblings.iter() {
        current = H::two_to_one_swap((ind_tracker & 1) == 1,&current, sibling);
        ind_tracker >>= 1;
    }
    current
}


pub fn compute_partial_merkle_root_from_leaves<
    Hash: PartialEq + Copy,
    Hasher: MerkleHasher<Hash>,
>(
    leaves: &[Hash],
) -> Hash {
    let mut current = leaves.to_vec();
    while current.len() > 1 {
        let mut next = vec![];
        for i in 0..current.len() / 2 {
            next.push(Hasher::two_to_one(&current[2 * i], &current[2 * i + 1]));
        }
        if current.len() % 2 == 1 {
            next.push(current[current.len() - 1]);
        }
        current = next;
    }
    current[0]
}