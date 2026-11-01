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
use crate::pdf_name::PdfName;
use crate::pdf_object::PdfObject;
use crate::reader::ObjectAccess;
use crate::{dictionary::Dictionary, stream::StreamObject};
use pdf_graphics::interval::Interval;
use std::any::TypeId;
use std::collections::HashMap;

/// The single argument passed to a typed decoder.
///
/// The caller keeps reference and depth scopes active while the decoder owns
/// this context. Shape conversions preserve that same traversal and report a
/// type mismatch instead of silently accepting an unrelated object kind.
pub struct ObjectContext<'read, A: ObjectAccess + ?Sized> {
    object: &'read ObjectVariant,
    /// The reference this object was requested through, when it was reached by one, with
    /// the cycle key of the decoder reading it.
    reference: Option<(ObjectId, TypeId)>,
    access: &'read mut A,
}

impl<'read, A: ObjectAccess + ?Sized> ObjectContext<'read, A> {
    /// Binds a direct object to an active access implementation.
    ///
    /// The object must already be resolved: a top-level reference is never followed.
    /// Custom access implementations must establish their traversal scope
    /// before constructing the context and retain it until decoding returns.
    pub fn new(object: &'read ObjectVariant, access: &'read mut A) -> Self {
        Self {
            object,
            reference: None,
            access,
        }
    }

    /// Binds an object resolved from a reference, keeping for [`Self::shared`] the
    /// requested id and the cycle key of the decoder reading it.
    pub(crate) fn referenced(
        object: &'read ObjectVariant,
        reference: (ObjectId, TypeId),
        access: &'read mut A,
    ) -> Self {
        Self {
            object,
            reference: Some(reference),
            access,
        }
    }

    /// Reads the bound object as a shared `T` through the typed cache.
    ///
    /// An object reached by reference shares that reference's typed entry, so a decoder
    /// can return a shared handle to the value it is decoding. The decoder's own eager
    /// scope ends first, so recursion back to this object meets the typed cache's pending
    /// handle rather than a cycle. A direct object gives an owned, ready handle.
    pub fn shared<T>(self) -> ReadResult<ObjectHandle<T>>
    where
        T: FromPdfObject + Send + Sync + 'static,
    {
        match self.reference {
            Some((id, key)) => self.access.read_shared_reached(id, key),
            None => self.access.read_shared(self.object),
        }
    }

    /// Returns the resolved object for custom shape inspection.
    ///
    /// The borrow outlives `&self`, so the object can be inspected while the
    /// context reads children.
    pub fn object(&self) -> &'read ObjectVariant {
        self.object
    }

    /// Decodes the bound object as `T` within the same traversal and decode level.
    ///
    /// For decoders whose value is spelled by another decoded type, such as a name enum
    /// reading its name bytes.
    #[inline]
    pub fn decode<T: FromPdfObject>(self) -> ReadResult<T> {
        T::from_pdf_object(self)
    }

    /// Requires a dictionary or stream and transfers the traversal into its dictionary context.
    ///
    /// Streams expose their metadata dictionary through the same entry-reading API.
    pub fn dictionary(self) -> ReadResult<DictionaryContext<'read, A>> {
        Ok(DictionaryContext {
            dictionary: self.object.dictionary()?,
            access: self.access,
        })
    }

    /// Requires an array and transfers the traversal into its context.
    pub fn array(self) -> ReadResult<ArrayContext<'read, A>> {
        match self.object {
            ObjectVariant::Array(array) => Ok(ArrayContext {
                array,
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
        match self.object {
            ObjectVariant::Stream(stream) => Ok(StreamContext {
                stream,
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
    dictionary: &'read Dictionary,
    access: &'read mut A,
}

impl<'read, A: ObjectAccess + ?Sized> DictionaryContext<'read, A> {
    /// Binds a dictionary the caller already holds to an active access implementation.
    ///
    /// Entries are read within the access's current traversal, as if the dictionary had
    /// been reached through it.
    pub fn new(dictionary: &'read Dictionary, access: &'read mut A) -> Self {
        Self { dictionary, access }
    }

    /// Returns whether `key` is present, whatever its value, including null or invalid ones.
    pub fn contains(&self, key: &[u8]) -> bool {
        self.dictionary.get(key).is_some()
    }

    /// Returns the entry names in key order.
    ///
    /// The names borrow the dictionary rather than the context, so entries can be read
    /// while iterating.
    pub fn keys(&self) -> impl Iterator<Item = &'read [u8]> + use<'read, A> {
        self.dictionary.keys()
    }

    /// Returns the number of the indirect object holding this dictionary, if it is one.
    pub fn object_number(&self) -> Option<usize> {
        self.dictionary.object_number
    }

    /// Returns the identifier of an entry stored as an indirect reference.
    pub fn reference(&self, key: &[u8]) -> Option<ObjectId> {
        match self.dictionary.get(key) {
            Some(ObjectVariant::Reference(id)) => Some(*id),
            _ => None,
        }
    }

    /// Resolves an entry's reference chain without entering its eager decoding scope.
    ///
    /// For an entry that may refer back to an object being decoded, whose eager read would
    /// reenter an object still being decoded.
    pub fn resolve_entry(&mut self, key: &[u8]) -> ReadResult<Option<PdfObject>> {
        let Some(object) = self.dictionary.get(key) else {
            return Ok(None);
        };
        self.access
            .resolve(object)
            .map(Some)
            .map_err(|source| ObjectReadError::At {
                location: ReadLocation::DictionaryKey(PdfName::from_bytes(key)),
                source: Box::new(source),
            })
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

    /// Reads an optional entry whose malformed value is treated as absent, for best-effort hints.
    pub fn lenient<T: FromPdfObject>(&mut self, key: &[u8]) -> Option<T> {
        self.optional(key).unwrap_or_default()
    }

    /// Decodes each entry of an optional sub-dictionary as `T` and maps it with `convert`,
    /// keyed by entry name.
    ///
    /// A missing or null sub-dictionary yields an empty map. Null entries and references to
    /// missing objects are skipped, since per PDF spec §7.3.7 and §7.3.10 they are equivalent
    /// to absent entries.
    pub fn optional_entries<T: FromPdfObject, R>(
        &mut self,
        key: &[u8],
        mut convert: impl FnMut(T) -> ReadResult<R>,
    ) -> ReadResult<HashMap<PdfName, R>> {
        let entries = self.with_optional_dictionary(key, |context, dictionary| {
            let mut entries = HashMap::with_capacity(dictionary.len());
            for (name, value) in dictionary.iter() {
                if let Some(entry) = context.access.read::<Option<T>>(value)? {
                    entries.insert(PdfName::from_bytes(name), convert(entry)?);
                }
            }
            Ok(entries)
        })?;
        Ok(entries.unwrap_or_default())
    }

    /// Decodes each entry of an optional sub-dictionary as `T`, keyed by entry name, skipping
    /// entries that do not decode as `T`, for namespaces that mix value kinds.
    ///
    /// A missing or null sub-dictionary yields an empty map, as with [`Self::optional_entries`].
    pub fn lenient_entries<T: FromPdfObject>(
        &mut self,
        key: &[u8],
    ) -> ReadResult<HashMap<PdfName, T>> {
        let entries = self.with_optional_dictionary(key, |context, dictionary| {
            Ok(dictionary
                .iter()
                .filter_map(|(name, value)| {
                    Some((PdfName::from_bytes(name), context.access.read(value).ok()?))
                })
                .collect())
        })?;
        Ok(entries.unwrap_or_default())
    }

    /// Runs `visit` on an optional sub-dictionary, borrowed from its resolved object rather
    /// than copied. A missing or null entry yields `None`.
    fn with_optional_dictionary<R>(
        &mut self,
        key: &[u8],
        visit: impl FnOnce(&mut Self, &Dictionary) -> ReadResult<R>,
    ) -> ReadResult<Option<R>> {
        let Some(object) = self.resolve_entry(key)? else {
            return Ok(None);
        };
        match object.value() {
            ObjectVariant::Null => Ok(None),
            ObjectVariant::Dictionary(dictionary) => visit(self, dictionary).map(Some),
            other => Err(ObjectReadError::At {
                location: ReadLocation::DictionaryKey(PdfName::from_bytes(key)),
                source: Box::new(ObjectReadError::TypeMismatch {
                    expected: ObjectKind::Dictionary,
                    actual: other.kind(),
                }),
            }),
        }
    }

    /// Reads an entry holding one value or an array of values, as a list.
    ///
    /// A missing or null entry yields an empty list, and null array members, such as
    /// references to missing objects, are skipped.
    pub fn optional_list<T: FromPdfObject>(&mut self, key: &[u8]) -> ReadResult<Vec<T>> {
        let Some(object) = self.dictionary.get(key) else {
            return Ok(Vec::new());
        };
        let resolved = self.access.resolve(object);
        let result = resolved.and_then(|resolved| match resolved.value() {
            ObjectVariant::Array(array) => array
                .iter()
                .filter_map(|member| self.access.read::<Option<T>>(member).transpose())
                .collect(),
            ObjectVariant::Null => Ok(Vec::new()),
            _ => Ok(vec![self.access.read(object)?]),
        });
        result.map_err(|source| ObjectReadError::At {
            location: ReadLocation::DictionaryKey(PdfName::from_bytes(key)),
            source: Box::new(source),
        })
    }

    /// Reads an optional flat `[min0 max0 min1 max1 ...]` array as one interval per pair.
    ///
    /// A missing or null entry is absent, and a trailing unpaired value is ignored.
    pub fn optional_intervals(&mut self, key: &[u8]) -> ReadResult<Option<Vec<Interval>>> {
        let Some(object) = self.dictionary.get(key) else {
            return Ok(None);
        };
        let resolved = self.access.resolve(object);
        let result = resolved.and_then(|resolved| match resolved.value() {
            ObjectVariant::Array(array) => ArrayContext::new(array, &mut *self.access)
                .read_intervals()
                .map(Some),
            ObjectVariant::Null => Ok(None),
            value => Err(ObjectReadError::TypeMismatch {
                expected: ObjectKind::Array,
                actual: value.kind(),
            }),
        });
        result.map_err(|source| ObjectReadError::At {
            location: ReadLocation::DictionaryKey(PdfName::from_bytes(key)),
            source: Box::new(source),
        })
    }

    /// Reads a required flat `[min0 max0 min1 max1 ...]` array as one interval per pair.
    ///
    /// A trailing unpaired value is ignored.
    pub fn required_intervals(&mut self, key: &[u8]) -> ReadResult<Vec<Interval>> {
        self.optional_intervals(key)?
            .ok_or_else(|| ObjectReadError::At {
                location: ReadLocation::DictionaryKey(PdfName::from_bytes(key)),
                source: Box::new(ObjectReadError::MissingDictionaryKey {
                    key: PdfName::from_bytes(key),
                }),
            })
    }
}

/// Provides bounds-checked typed array reads in source order.
pub struct ArrayContext<'read, A: ObjectAccess + ?Sized> {
    array: &'read [ObjectVariant],
    access: &'read mut A,
}

impl<'read, A: ObjectAccess + ?Sized> ArrayContext<'read, A> {
    /// Binds a direct sequence of values the caller already holds to an active access
    /// implementation.
    ///
    /// Elements are read within the access's current traversal, as if the array had
    /// been reached through it.
    pub fn new(array: &'read [ObjectVariant], access: &'read mut A) -> Self {
        Self { array, access }
    }

    /// Returns the immutable elements for length checks or custom inspection.
    ///
    /// The borrow outlives `&self`, so elements can be held while the context reads them.
    pub fn array(&self) -> &'read [ObjectVariant] {
        self.array
    }

    /// Returns the identifier of an element stored as an indirect reference.
    pub fn reference_at(&self, index: usize) -> Option<ObjectId> {
        match self.array.get(index) {
            Some(ObjectVariant::Reference(id)) => Some(*id),
            _ => None,
        }
    }

    /// Reads an element, reporting an out-of-bounds error for an invalid index.
    #[inline]
    pub fn at<T: FromPdfObject>(&mut self, index: usize) -> ReadResult<T> {
        let Some(object) = self.array.get(index) else {
            return Err(ObjectReadError::At {
                location: ReadLocation::ArrayIndex(index),
                source: Box::new(ObjectReadError::ArrayIndexOutOfBounds {
                    index,
                    length: self.array.len(),
                }),
            });
        };
        self.access
            .read(object)
            .map_err(|source| ObjectReadError::At {
                location: ReadLocation::ArrayIndex(index),
                source: Box::new(source),
            })
    }

    /// Reads an optional element: an index past the end or a value resolving to null is absent.
    pub fn optional_at<T: FromPdfObject>(&mut self, index: usize) -> ReadResult<Option<T>> {
        let Some(object) = self.array.get(index) else {
            return Ok(None);
        };
        self.access
            .read(object)
            .map_err(|source| ObjectReadError::At {
                location: ReadLocation::ArrayIndex(index),
                source: Box::new(source),
            })
    }

    /// Eagerly decodes every element in source order, stopping at the first error.
    pub fn read_all<T: FromPdfObject>(&mut self) -> ReadResult<Vec<T>> {
        let mut values = Vec::with_capacity(self.array.len());
        for (index, object) in self.array.iter().enumerate() {
            values.push(
                self.access
                    .read(object)
                    .map_err(|source| ObjectReadError::At {
                        location: ReadLocation::ArrayIndex(index),
                        source: Box::new(source),
                    })?,
            );
        }
        Ok(values)
    }

    /// Reads the elements as a flat `[min0 max0 min1 max1 ...]` list, one interval per pair.
    ///
    /// A trailing unpaired value is ignored.
    pub fn read_intervals(&mut self) -> ReadResult<Vec<Interval>> {
        (0..self.array.len().saturating_sub(1))
            .step_by(2)
            .map(|min| {
                Ok(Interval::new(
                    self.at(min)?,
                    self.at(min.saturating_add(1))?,
                ))
            })
            .collect()
    }

    /// Decodes the first `N` elements in source order, validating the length first.
    ///
    /// Trailing elements are ignored, like the `[T; N]` decoder.
    #[inline]
    pub fn read_array<T: FromPdfObject + Default, const N: usize>(&mut self) -> ReadResult<[T; N]> {
        if self.array.len() < N {
            return Err(crate::object_error::ObjectError::InvalidArrayLength {
                expected: N,
                found: self.array.len(),
            }
            .into());
        }
        let mut values = std::array::from_fn(|_| T::default());
        for (index, (value, object)) in values.iter_mut().zip(self.array).enumerate() {
            *value = self
                .access
                .read(object)
                .map_err(|source| ObjectReadError::At {
                    location: ReadLocation::ArrayIndex(index),
                    source: Box::new(source),
                })?;
        }
        Ok(values)
    }
}

/// Exposes a stream's metadata and bytes without applying filters.
///
/// Encoded and decoded data remain distinguished by the model. Filter execution
/// can be provided by client services and remains in the filter and document crates.
pub struct StreamContext<'read, A: ObjectAccess + ?Sized> {
    stream: &'read StreamObject,
    access: &'read mut A,
}

impl<'read, A: ObjectAccess + ?Sized> StreamContext<'read, A> {
    /// Returns the stream handle, including bytes and filter-state metadata.
    ///
    /// The borrow outlives `&self`, so the stream can be held while the context reads children.
    pub fn stream(&self) -> &'read StreamObject {
        self.stream
    }

    /// Reborrows the active traversal to read the stream's dictionary entries.
    pub fn dictionary(&mut self) -> DictionaryContext<'_, A> {
        DictionaryContext {
            dictionary: &self.stream.dictionary,
            access: &mut *self.access,
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

            /// Resolves a raw child without entering its eager decoding scope.
            pub fn resolve(
                &mut self,
                object: &ObjectVariant,
            ) -> ReadResult<crate::pdf_object::PdfObject> {
                self.access.resolve(object)
            }

            /// Returns whether an object resolves to null.
            ///
            /// A reference to a nonexistent object resolves to null (PDF spec §7.3.10), and
            /// per §7.3.7 a null dictionary value is equivalent to an absent entry.
            pub fn is_absent(&mut self, object: &ObjectVariant) -> ReadResult<bool> {
                Ok(self.access.resolve(object)?.kind() == ObjectKind::Null)
            }

            /// Reborrows the active traversal for decoders that take an access directly.
            pub fn access(&mut self) -> &mut A {
                self.access
            }

            /// Returns the reader's shared content-stream ID allocator.
            pub fn content_stream_ids(&self) -> &ContentStreamIdAllocator {
                self.access.content_stream_ids()
            }

            /// Eagerly decodes an indirect object within the current traversal.
            pub fn read_indirect<T: FromPdfObject>(&mut self, id: ObjectId) -> ReadResult<T> {
                self.access.read_indirect(id)
            }
        }
    };
}

context_access!(ObjectContext);
context_access!(DictionaryContext);
context_access!(ArrayContext);
context_access!(StreamContext);
