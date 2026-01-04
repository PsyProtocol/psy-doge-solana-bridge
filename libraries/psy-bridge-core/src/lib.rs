pub mod common_types;
pub mod crypto;
pub mod txo_constants;
pub mod error;
pub mod serde_arrays;
pub mod header;
use macro_rules_attribute::attribute_alias;

// Define the alias for the entire block of attributes.
// The macro `CommonDerives` becomes the alias.
attribute_alias! {
    #[apply(DeriveCopySerializeDefaultReprC)] =
        #[cfg_attr(feature = "serialize_serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serialize_borsh", derive(borsh::BorshSerialize, borsh::BorshDeserialize))]
        #[cfg_attr(feature = "serialize_speedy", derive(speedy::Readable, speedy::Writable))]
        #[cfg_attr(feature = "serialize_bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
        #[derive(PartialEq, Clone, Debug, Eq, Ord, PartialOrd, Copy, Hash, Default)]
        #[repr(C)];
}

attribute_alias! {
    #[apply(DeriveCopySerializeReprC)] =
        #[cfg_attr(feature = "serialize_serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serialize_borsh", derive(borsh::BorshSerialize, borsh::BorshDeserialize))]
        #[cfg_attr(feature = "serialize_speedy", derive(speedy::Readable, speedy::Writable))]
        #[cfg_attr(feature = "serialize_bytemuck", derive(bytemuck::Pod, bytemuck::Zeroable))]
        #[derive(PartialEq, Clone, Debug, Eq, Ord, PartialOrd, Copy, Hash)]
        #[repr(C)];
}
