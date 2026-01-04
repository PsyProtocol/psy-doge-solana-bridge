
use crate::common_types::QHash160;

use super::{
    ripemd160_impl::hash_impl_ripemd160_bytes, sha256_impl::hash_impl_sha256_bytes, traits::{iterate_merkle_hasher, BytesHasher, MerkleHasher, MerkleZeroHasher}
};

#[derive(Clone, Copy)]
pub struct QRipemd160Hasher;

impl BytesHasher<QHash160> for QRipemd160Hasher {
    fn hash_bytes(data: &[u8]) -> QHash160 {
        hash_impl_ripemd160_bytes(data)
    }
}
impl MerkleHasher<QHash160> for QRipemd160Hasher {
    fn two_to_one(left: &QHash160, right: &QHash160) -> QHash160 {
        let mut bytes = [0u8; 40];
        bytes[0..20].copy_from_slice(left);
        bytes[20..40].copy_from_slice(right);
        hash_impl_ripemd160_bytes(&bytes)
    }
}
impl MerkleZeroHasher<QHash160> for QRipemd160Hasher {
    fn get_zero_hash(reverse_level: usize) -> QHash160 {
        iterate_merkle_hasher::<QHash160, Self>([0u8; 20], reverse_level)
    }
}



#[derive(Clone, Copy)]
pub struct QBTCHash160Hasher;

impl BytesHasher<QHash160> for QBTCHash160Hasher {
    fn hash_bytes(data: &[u8]) -> QHash160 {
        hash_impl_ripemd160_bytes(&hash_impl_sha256_bytes(data))
    }
}
impl MerkleHasher<QHash160> for QBTCHash160Hasher {
    fn two_to_one(left: &QHash160, right: &QHash160) -> QHash160 {
        let mut bytes = [0u8; 40];
        bytes[0..20].copy_from_slice(left);
        bytes[20..40].copy_from_slice(right);
        hash_impl_ripemd160_bytes(&hash_impl_sha256_bytes(&bytes))
    }
}
impl MerkleZeroHasher<QHash160> for QBTCHash160Hasher {
    fn get_zero_hash(reverse_level: usize) -> QHash160 {
        iterate_merkle_hasher::<QHash160, Self>([0u8; 20], reverse_level)
    }
}
