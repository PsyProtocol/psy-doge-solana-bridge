
#[cfg(all(feature = "sp1", feature = "sha2"))]
compile_error!("Feature 'sp1' and 'sha2' cannot be enabled at the same time.");

#[cfg(all(feature = "sp1", feature = "solprogram"))]
compile_error!("Feature 'sp1' and 'solprogram' cannot be enabled at the same time.");
#[cfg(all(target_os = "solana", feature = "sha2"))]
compile_error!("Feature 'sha2' cannot be enabled when targeting Solana.");

#[cfg(all(feature = "sha2", feature = "solprogram"))]
compile_error!("Feature 'sha2' and 'solprogram' cannot be enabled at the same time.");

#[cfg(not(any(feature = "sp1", feature = "sha2", feature = "solprogram")))]
compile_error!("You must enable exactly one hashing backend: 'sp1', 'sha2', or 'solprogram'.");
#[cfg(all(feature = "sha2", not(feature = "solprogram")))]
use sha2::{Digest, Sha256};
#[cfg(feature = "sp1")]
use sha2_v0_10_9::Sha256;

use crate::common_types::QHash256;

#[cfg(all(any(feature = "sha2", feature= "sp1"), not(feature = "solprogram")))]
#[inline]
pub fn hash_impl_sha256_bytes(bytes: &[u8]) -> QHash256 {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    result.into()
}
#[cfg(not(any(feature = "solprogram", feature = "sha2", feature= "sp1")))]
#[inline]
pub fn hash_impl_sha256_bytes(bytes: &[u8]) -> QHash256 {
    panic!("SHA256 hashing not supported in this configuration");
}

#[cfg(feature = "solprogram")]
#[inline]
pub fn hash_impl_sha256_bytes(bytes: &[u8]) -> QHash256 {
    solana_program::hash::hash(bytes).to_bytes()
}

#[cfg(all(any(feature = "sha2", feature= "sp1"), not(feature = "solprogram")))]
#[inline]
pub fn hashv_impl_sha256_bytes(bytes_vec: &[&[u8]]) -> QHash256 {
    let mut hasher = Sha256::new();
    for bytes in bytes_vec {
        hasher.update(bytes);
    }
    let result = hasher.finalize();
    result.into()
}

#[cfg(feature = "solprogram")]
#[inline]
pub fn hashv_impl_sha256_bytes(bytes_vec: &[&[u8]]) -> QHash256 {
    solana_program::hash::hashv(bytes_vec).to_bytes()
}



#[cfg(not(any(feature = "solprogram", feature = "sha2", feature= "sp1")))]
#[inline]
pub fn hashv_impl_sha256_bytes(bytes: &[u8]) -> QHash256 {
    panic!("SHA256 hashing not supported in this configuration");
}

#[inline]
pub fn hash_impl_sha256_two_to_one_bytes_buf(buf: &mut [u8; 64], left: &QHash256, right: &QHash256) -> QHash256 {
    buf[..32].copy_from_slice(left.as_ref());
    buf[32..].copy_from_slice(right.as_ref());
    hash_impl_sha256_bytes(buf)
}


#[inline]
pub fn hash_impl_sha256_two_to_one_bytes(left: &QHash256, right: &QHash256) -> QHash256 {
    hash_impl_sha256_bytes(&[&left[..], &right[..]].concat())
}

#[inline]
pub fn hash_impl_btc_hash256_two_to_one_bytes(left: &QHash256, right: &QHash256) -> QHash256 {
    hash_impl_sha256_bytes(&hashv_impl_sha256_bytes(&[&left[..], &right[..]]))

}


#[inline]
pub fn hash_impl_sha256_hash_two_buffers_concat(left: &[u8], right: &[u8]) -> QHash256 {
    hashv_impl_sha256_bytes(&[left, right])
}


#[inline]
pub fn hash_impl_sha256_hash_three_buffers_concat(a: &[u8], b: &[u8], c: &[u8]) -> QHash256 {
    hashv_impl_sha256_bytes(&[a, b, c])
}
#[inline]
pub fn hash_impl_sha256_hash_four_buffers_concat(a: &[u8], b: &[u8], c: &[u8], d: &[u8]) -> QHash256 {
    hashv_impl_sha256_bytes(&[a, b, c, d])
}


#[inline]
pub fn hash_impl_sha256_compute_merkle_root(value: &[u8; 32], index: u64, siblings: &[[u8; 32]]) -> QHash256 {
    assert!(siblings.len() <= 64);
    let mut current_hash = *value;
    let mut idx = index;
    for sibling in siblings {
        current_hash = if idx & 1 == 0 {
            hash_impl_sha256_two_to_one_bytes(&current_hash, sibling)
        }else{
            hash_impl_sha256_two_to_one_bytes(sibling, &current_hash)
        };
        idx >>= 1;
    }
    assert!(idx == 0);
    current_hash
}