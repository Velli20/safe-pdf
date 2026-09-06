//! Errors reported by object acquisition, resolution, and typed decoding.

use std::error::Error;
use std::sync::Arc;

use thiserror::Error;

use crate::object_id::ObjectId;
use crate::object_kind::ObjectKind;
use crate::pdf_name::PdfName;

/// A thread-safe dynamically typed error received from an extension point.
pub type BoxedError = Box<dyn Error + Send + Sync + 'static>;

/// A result returned by PDF object reading operations.
pub type ReadResult<T> = Result<T, ObjectReadError>;

/// Identifies the container edge at which a nested read failed.
///
/// Nested At errors preserve the outer-to-inner dictionary and array path
/// without copying diagnostic paths on successful reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadLocation {
    /// An entry selected by its decoded PDF name bytes.
    DictionaryKey(PdfName),
    /// An element selected by its zero-based array position.
    ArrayIndex(usize),
}

/// Describes failures encountered while reading or interpreting an object.
#[derive(Debug, Error)]
pub enum ObjectReadError {
    /// A nested read failed at a dictionary key or array index.
    #[error("object read failed at {location:?}")]
    At {
        /// The container edge traversed by the failed read.
        location: ReadLocation,
        /// The original nested error.
        #[source]
        source: Box<ObjectReadError>,
    },
    /// A checked direct-object conversion received an unresolved reference.
    #[error("indirect reference {object_id:?} must be resolved before decoding")]
    UnresolvedReference {
        /// The ID that still requires source resolution.
        object_id: ObjectId,
    },
    /// The configured source failed while acquiring an indirect object.
    #[error("failed to read indirect object {object_id:?}")]
    Source {
        /// The object requested from the source.
        object_id: ObjectId,
        /// The source-specific failure.
        #[source]
        source: BoxedError,
    },
    /// The configured cache failed while looking up or publishing an object.
    #[error("object cache failed while handling {object_id:?}")]
    Cache {
        /// The object being handled by the cache.
        object_id: ObjectId,
        /// The cache-specific failure.
        #[source]
        source: BoxedError,
    },
    /// The source had no value for a referenced indirect object.
    #[error("indirect object {object_id:?} was not found")]
    MissingObject {
        /// The unresolved object identifier.
        object_id: ObjectId,
    },
    /// Reference traversal revisited an object active in the current read.
    #[error("cyclic reference encountered at {repeated:?}; active path: {path:?}")]
    CyclicReference {
        /// The object identifier repeated in the active path.
        repeated: ObjectId,
        /// The active traversal path at the point the cycle was detected.
        path: Vec<ObjectId>,
    },
    /// Reference traversal exceeded the configured depth limit.
    #[error("reference depth exceeded maximum {maximum}; active path: {path:?}")]
    ReferenceDepthExceeded {
        /// The maximum permitted number of simultaneously active references.
        maximum: usize,
        /// The path whose next reference would exceed the limit.
        path: Vec<ObjectId>,
    },
    /// Nested decoding, including direct containers, exceeded its depth limit.
    #[error("decoding depth exceeded maximum {maximum}")]
    DecodeDepthExceeded {
        /// The permitted number of simultaneously active decoder calls.
        maximum: usize,
    },
    /// Internal typed-cache synchronization or checked type recovery failed.
    #[error("typed cache failed during {operation}")]
    TypedCache {
        /// The operation whose lock or checked downcast failed.
        operation: &'static str,
        /// The underlying cache failure.
        #[source]
        source: BoxedError,
    },
    /// Value access required completion while another decoder owns the entry.
    #[error("typed object {object_id:?} is still being decoded")]
    ObjectPending {
        /// The entry whose value is not yet available.
        object_id: ObjectId,
    },
    /// The reader owning an indirect typed entry has been destroyed.
    #[error("typed object handle {object_id:?} has expired")]
    HandleExpired {
        /// The identity retained by the expired handle.
        object_id: ObjectId,
    },
    /// A load reservation ended without publishing a value or explicit failure.
    #[error("typed object load {object_id:?} was abandoned")]
    LoadAborted {
        /// The entry whose reservation was abandoned.
        object_id: ObjectId,
    },
    /// A previously attempted typed decode failed.
    #[error("cached decoding of {object_id:?} as {target} failed")]
    CachedDecode {
        /// The requested indirect object.
        object_id: ObjectId,
        /// The Rust output type associated with the failed entry.
        target: &'static str,
        /// The shared original cause, retained for all existing handles.
        #[source]
        source: Arc<ObjectReadError>,
    },
    /// A typed decoder received a different PDF value kind than it requires.
    #[error("type mismatch: expected {expected:?}, found {actual:?}")]
    TypeMismatch {
        /// The value kind required by the decoder.
        expected: ObjectKind,
        /// The kind actually found after reference resolution.
        actual: ObjectKind,
    },
    /// A required dictionary entry was absent.
    #[error("required dictionary key {key:?} was not found")]
    MissingDictionaryKey {
        /// The missing PDF name.
        key: PdfName,
    },
    /// A requested array index was outside the available range.
    #[error("array index {index} is out of bounds for length {length}")]
    ArrayIndexOutOfBounds {
        /// The index requested by the decoder.
        index: usize,
        /// The number of elements in the array.
        length: usize,
    },
    /// A client-defined typed decoder reported a domain-specific failure.
    #[error("failed to decode PDF object as {target}")]
    Decode {
        /// The Rust type or domain concept being decoded.
        target: &'static str,
        /// The decoder-specific failure.
        #[source]
        source: BoxedError,
    },
}

impl From<crate::object_error::ObjectError> for ObjectReadError {
    fn from(source: crate::object_error::ObjectError) -> Self {
        Self::Decode {
            target: "PDF object",
            source: Box::new(source),
        }
    }
}

impl From<crate::ContentStreamIdExhausted> for ObjectReadError {
    fn from(source: crate::ContentStreamIdExhausted) -> Self {
        Self::Decode {
            target: "PDF content stream",
            source: Box::new(source),
        }
    }
}
