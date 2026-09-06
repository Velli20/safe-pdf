//! Object-bound decoding contexts that carry content-stream IDs and recursive access.
//!
//! A context owns a cheap model handle and borrows the current traversal.
//! Reborrowing child contexts prevents independent mutable access to that
//! traversal while a child is active.

use crate::ContentStreamIdAllocator;
use crate::decode::FromPdfObject;
use crate::error::{ObjectReadError, ReadLocation, ReadResult};
use crate::handle::ObjectHandle;
use crate::object_id::ObjectId;
use crate::object_kind::ObjectKind;
use crate::object_variant::ObjectVariant;
use crate::pdf_array::PdfArray;
use crate::pdf_name::PdfName;
use crate::reader::ObjectAccess;
use crate::resolved_object::ResolvedObject;
use crate::{dictionary::Dictionary, stream::StreamObject};

/// The single argument passed to a typed decoder.
///
/// The caller keeps reference and depth scopes active while the decoder owns
/// this context. Shape conversions preserve that same traversal and report a
/// type mismatch instead of silently accepting an unrelated object kind.
pub struct ObjectContext<'read, A: ObjectAccess + ?Sized> {
    object: ResolvedObject,
    access: &'read mut A,
}

impl<'read, A: ObjectAccess + ?Sized> ObjectContext<'read, A> {
    /// Binds a validated direct object to an active access implementation.
    ///
    /// Custom access implementations must establish their traversal scope
    /// before constructing the context and retain it until decoding returns.
    pub fn new(object: ResolvedObject, access: &'read mut A) -> Self {
        Self { object, access }
    }

    /// Returns the resolved object for custom shape inspection.
    pub fn object(&self) -> &ResolvedObject {
        &self.object
    }

    /// Requires a dictionary or stream and transfers the traversal into its dictionary context.
    ///
    /// Streams expose their metadata dictionary through the same entry-reading API.
    pub fn dictionary(self) -> ReadResult<DictionaryContext<'read, A>> {
        match self.object.value() {
            ObjectVariant::Dictionary(value) => Ok(DictionaryContext {
                dictionary: value.clone(),
                access: self.access,
            }),
            ObjectVariant::Stream(value) => Ok(DictionaryContext {
                dictionary: value.dictionary.clone(),
                access: self.access,
            }),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Dictionary,
                actual: self.object.kind(),
            }),
        }
    }

    /// Requires an array and transfers the traversal into its context.
    pub fn array(self) -> ReadResult<ArrayContext<'read, A>> {
        match self.object.value() {
            ObjectVariant::Array(value) => Ok(ArrayContext {
                array: PdfArray::new(value.clone()),
                access: self.access,
            }),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Array,
                actual: self.object.kind(),
            }),
        }
    }

    /// Requires a stream and transfers the traversal into its context.
    pub fn stream(self) -> ReadResult<StreamContext<'read, A>> {
        match self.object.value() {
            ObjectVariant::Stream(value) => Ok(StreamContext {
                stream: value.clone(),
                access: self.access,
            }),
            _ => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Stream,
                actual: self.object.kind(),
            }),
        }
    }
}

/// Provides typed dictionary entry access using the decoder's active session.
///
/// Optional reads treat a missing key or a value resolving to PDF null as
/// absent. Missing indirect objects, cycles, and invalid present values remain
/// errors. Required reads reject a missing key; null follows the target decoder's
/// contract, so a decoder explicitly accepting null can still read it.
pub struct DictionaryContext<'read, A: ObjectAccess + ?Sized> {
    dictionary: Dictionary,
    access: &'read mut A,
}

impl<A: ObjectAccess + ?Sized> DictionaryContext<'_, A> {
    /// Returns the immutable dictionary for iteration or custom entry selection.
    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    /// Reads a required entry and annotates failures with its byte-oriented key.
    pub fn required<T: FromPdfObject>(&mut self, key: &[u8]) -> ReadResult<T> {
        let result = match self.dictionary.get(key) {
            Some(object) => self.access.read(object),
            None => Err(ObjectReadError::MissingDictionaryKey {
                key: PdfName::from_bytes(key),
            }),
        };
        result.map_err(|source| ObjectReadError::At {
            location: ReadLocation::DictionaryKey(PdfName::from_bytes(key)),
            source: Box::new(source),
        })
    }

    /// Reads an optional entry, resolving references before testing for null.
    pub fn optional<T: FromPdfObject>(&mut self, key: &[u8]) -> ReadResult<Option<T>> {
        let Some(object) = self.dictionary.get(key) else {
            return Ok(None);
        };
        self.access
            .read(object)
            .map_err(|source| ObjectReadError::At {
                location: ReadLocation::DictionaryKey(PdfName::from_bytes(key)),
                source: Box::new(source),
            })
    }

    /// Reads a required entry as a shared value or a pending recursive handle.
    pub fn required_shared<T>(&mut self, key: &[u8]) -> ReadResult<ObjectHandle<T>>
    where
        T: FromPdfObject + Send + Sync + 'static,
    {
        let result = match self.dictionary.get(key) {
            Some(object) => self.access.read_shared(object),
            None => Err(ObjectReadError::MissingDictionaryKey {
                key: PdfName::from_bytes(key),
            }),
        };
        result.map_err(|source| ObjectReadError::At {
            location: ReadLocation::DictionaryKey(PdfName::from_bytes(key)),
            source: Box::new(source),
        })
    }

    /// Reads a shared entry when present and non-null.
    ///
    /// Null detection must preserve the original indirect identity used by the
    /// typed cache. A pending typed entry remains a handle, not an absent value.
    /// Inspect only the raw top-level reference chain for null, with chain-local
    /// cycle checks; do not enter a second eager decoding scope for that probe.
    pub fn optional_shared<T>(&mut self, key: &[u8]) -> ReadResult<Option<ObjectHandle<T>>>
    where
        T: FromPdfObject + Send + Sync + 'static,
    {
        let Some(object) = self.dictionary.get(key) else {
            return Ok(None);
        };
        let result = (|| {
            if self.access.resolve(object)?.kind() == ObjectKind::Null {
                return Ok(None);
            }
            self.access.read_shared(object).map(Some)
        })();
        result.map_err(|source| ObjectReadError::At {
            location: ReadLocation::DictionaryKey(PdfName::from_bytes(key)),
            source: Box::new(source),
        })
    }
}

/// Provides bounds-checked typed array reads in source order.
pub struct ArrayContext<'read, A: ObjectAccess + ?Sized> {
    array: PdfArray,
    access: &'read mut A,
}

impl<A: ObjectAccess + ?Sized> ArrayContext<'_, A> {
    /// Returns the immutable array for length checks or custom inspection.
    pub fn array(&self) -> &PdfArray {
        &self.array
    }

    /// Reads an element, reporting an out-of-bounds error for an invalid index.
    pub fn at<T: FromPdfObject>(&mut self, index: usize) -> ReadResult<T> {
        let result = match self.array.get(index) {
            Some(object) => self.access.read(object),
            None => Err(ObjectReadError::ArrayIndexOutOfBounds {
                index,
                length: self.array.len(),
            }),
        };
        result.map_err(|source| ObjectReadError::At {
            location: ReadLocation::ArrayIndex(index),
            source: Box::new(source),
        })
    }

    /// Reads an element through typed caching while retaining array diagnostics.
    pub fn shared_at<T>(&mut self, index: usize) -> ReadResult<ObjectHandle<T>>
    where
        T: FromPdfObject + Send + Sync + 'static,
    {
        let result = match self.array.get(index) {
            Some(object) => self.access.read_shared(object),
            None => Err(ObjectReadError::ArrayIndexOutOfBounds {
                index,
                length: self.array.len(),
            }),
        };
        result.map_err(|source| ObjectReadError::At {
            location: ReadLocation::ArrayIndex(index),
            source: Box::new(source),
        })
    }

    /// Eagerly decodes every element in source order, stopping at the first error.
    pub fn read_all<T: FromPdfObject>(&mut self) -> ReadResult<Vec<T>> {
        (0..self.array.len()).map(|index| self.at(index)).collect()
    }
}

/// Exposes a stream's metadata and bytes without applying filters.
///
/// Encoded and decoded data remain distinguished by the model. Filter execution
/// can be provided by client services and remains in the filter and document crates.
pub struct StreamContext<'read, A: ObjectAccess + ?Sized> {
    stream: StreamObject,
    access: &'read mut A,
}

impl<A: ObjectAccess + ?Sized> StreamContext<'_, A> {
    /// Returns the stream handle, including bytes and filter-state metadata.
    pub fn stream(&self) -> &StreamObject {
        &self.stream
    }

    /// Reborrows the active traversal to read the stream's dictionary entries.
    pub fn dictionary(&mut self) -> DictionaryContext<'_, A> {
        DictionaryContext {
            dictionary: self.stream.dictionary.clone(),
            access: self.access,
        }
    }
}

macro_rules! context_access {
    ($context:ident) => {
        impl<A: ObjectAccess + ?Sized> $context<'_, A> {
            /// Eagerly decodes another object within the same traversal.
            pub fn read<T: FromPdfObject>(&mut self, object: &ObjectVariant) -> ReadResult<T> {
                self.access.read(object)
            }

            /// Reads another object through typed caching within the same traversal.
            pub fn read_shared<T>(&mut self, object: &ObjectVariant) -> ReadResult<ObjectHandle<T>>
            where
                T: FromPdfObject + Send + Sync + 'static,
            {
                self.access.read_shared(object)
            }
            /// Resolves a raw child without entering its eager decoding scope.
            pub fn resolve(&mut self, object: &ObjectVariant) -> ReadResult<ResolvedObject> {
                self.access.resolve(object)
            }

            /// Borrows the source for leaf-level raw object inspection.
            pub fn source(&self) -> &dyn crate::object_resolver::ObjectResolver {
                self.access.source()
            }

            /// Returns the reader's shared content-stream ID allocator.
            pub fn content_stream_ids(&self) -> &ContentStreamIdAllocator {
                self.access.content_stream_ids()
            }

            /// Eagerly decodes an indirect object within the current traversal.
            pub fn read_indirect<T: FromPdfObject>(&mut self, id: ObjectId) -> ReadResult<T> {
                self.access.read_indirect(id)
            }

            /// Reads a shared indirect value without waiting for a pending entry.
            pub fn read_shared_indirect<T>(&mut self, id: ObjectId) -> ReadResult<ObjectHandle<T>>
            where
                T: FromPdfObject + Send + Sync + 'static,
            {
                self.access.read_shared_indirect(id)
            }
        }
    };
}

context_access!(ObjectContext);
context_access!(DictionaryContext);
context_access!(ArrayContext);
context_access!(StreamContext);
