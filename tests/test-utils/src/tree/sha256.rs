use psy_bridge_core::{common_types::QHash256, crypto::hash::{sha256::SHA256_ZERO_HASHES, sha256_impl::hash_impl_sha256_two_to_one_bytes, traits::{MerkleHasher, MerkleZeroHasher}}};

pub struct CoreSha256Hasher {

}
impl MerkleHasher<QHash256> for CoreSha256Hasher {
    fn two_to_one(left: &QHash256, right: &QHash256) -> QHash256 {
        hash_impl_sha256_two_to_one_bytes(left, right)
    }
}


impl MerkleZeroHasher<QHash256> for CoreSha256Hasher {
    fn get_zero_hash(reverse_level: usize) -> QHash256 {
        SHA256_ZERO_HASHES[reverse_level]
    }
}