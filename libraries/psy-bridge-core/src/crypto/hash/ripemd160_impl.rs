
use ripemd::{Digest, Ripemd160};
use crate::crypto::hash::sha256_impl::hash_impl_sha256_bytes;


pub fn hash_impl_ripemd160_bytes(bytes: &[u8]) -> [u8; 20] {
    let mut hasher = Ripemd160::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    result.into()
}

pub fn hash_impl_btc_hash160_bytes(bytes: &[u8]) -> [u8; 20] {
    let sha256_hash = hash_impl_sha256_bytes(bytes);
    hash_impl_ripemd160_bytes(&sha256_hash)
}