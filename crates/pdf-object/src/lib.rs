pub mod cross_reference_table;
pub mod dictionary;
pub mod error;
pub mod indirect_object;
pub mod object_collection;
pub mod object_variant;
pub mod stream;
pub mod trailer;
pub mod traits;
pub mod version;

pub use object_variant::ObjectVariant;

// Note: ObjectVariant implementation moved to `object_variant.rs`.
