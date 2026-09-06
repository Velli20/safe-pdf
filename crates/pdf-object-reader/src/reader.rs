//! Reader orchestration and session-local traversal state.

use std::collections::HashSet;
use std::num::NonZeroUsize;

use crate::ContentStreamIdAllocator;
use crate::cache::Reservation;
use crate::cache::{ConcurrentObjectCache, ObjectCache, TypedObjectCache};
use crate::context::ObjectContext;
use crate::decode::FromPdfObject;
use crate::error::{ObjectReadError, ReadResult};
use crate::handle::ObjectHandle;
use crate::object_id::ObjectId;
use crate::object_variant::ObjectVariant;
use crate::pdf_object::PdfObject;
use crate::resolved_object::ResolvedObject;
use crate::source::ObjectSource;
use std::sync::Arc;

/// Bounds reference traversal and nested decoding, including direct containers.
///
/// Both limits default to 128. They count active stack depth, not total objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadLimits {
    max_reference_depth: NonZeroUsize,
    max_decode_depth: NonZeroUsize,
}

impl ReadLimits {
    /// Configures independent limits for active references and decoder invocations.
    pub fn new(max_reference_depth: NonZeroUsize, max_decode_depth: NonZeroUsize) -> Self {
        Self {
            max_reference_depth,
            max_decode_depth,
        }
    }

    /// Returns the maximum number of simultaneously active indirect references.
    pub fn max_reference_depth(self) -> NonZeroUsize {
        self.max_reference_depth
    }

    /// Returns the maximum nesting of decoders, including direct array elements.
    pub fn max_decode_depth(self) -> NonZeroUsize {
        self.max_decode_depth
    }
}

impl Default for ReadLimits {
    fn default() -> Self {
        Self::new(
            NonZeroUsize::new(128).unwrap_or(NonZeroUsize::MIN),
            NonZeroUsize::new(128).unwrap_or(NonZeroUsize::MIN),
        )
    }
}

/// Owns one immutable document snapshot, its caches, and content-stream IDs.
///
/// A reader is shareable when its components are `Send + Sync`. Sessions keep
/// their own traversal state; the reader never holds a cache lock while invoking
/// a source or a decoder. The typed cache is private and lives with this reader.
///
/// Content-stream IDs are unique across all sessions of this reader.
/// Configuration affecting decoded values must remain stable for the reader's
/// lifetime. Use a new reader for a different snapshot or configuration, and
/// distinct decoder newtypes for different interpretations of the same object.
///
/// The raw cache supplied to this reader must belong exclusively to this document
/// snapshot. It must not reuse entries from other documents with overlapping IDs.
pub struct ObjectReader<S, C = ConcurrentObjectCache> {
    source: S,
    content_stream_ids: ContentStreamIdAllocator,
    raw_cache: C,
    typed_cache: TypedObjectCache,
    limits: ReadLimits,
}

impl<S> ObjectReader<S> {
    /// Creates a reader with empty caches, default limits, and fresh content IDs.
    pub fn new(source: S) -> Self {
        Self::with_components(source, ConcurrentObjectCache::new(), ReadLimits::default())
    }
}

impl<S, C> ObjectReader<S, C> {
    /// Configures storage, raw caching, and traversal limits with fresh content IDs.
    ///
    /// A fresh typed cache is always created; typed entries cannot be injected
    /// from another reader or another snapshot.
    pub fn with_components(source: S, raw_cache: C, limits: ReadLimits) -> Self {
        Self {
            source,
            content_stream_ids: ContentStreamIdAllocator::new(),
            raw_cache,
            typed_cache: TypedObjectCache::new(),
            limits,
        }
    }

    /// Returns the acquisition source without exposing traversal state.
    pub fn source(&self) -> &S {
        &self.source
    }

    /// Returns the atomic content-stream ID allocator shared by every session.
    pub fn content_stream_ids(&self) -> &ContentStreamIdAllocator {
        &self.content_stream_ids
    }

    /// Returns the reference and decoding depth limits.
    pub fn limits(&self) -> ReadLimits {
        self.limits
    }

    /// Starts a fresh traversal while retaining this reader's caches and content IDs.
    pub fn session(&self) -> ReadSession<'_, S, C> {
        ReadSession {
            reader: self,
            active_path: Vec::new(),
            active_ids: HashSet::new(),
            decode_depth: 0,
        }
    }
}

impl<S, C> ObjectReader<S, C>
where
    S: ObjectSource,
    C: ObjectCache,
{
    /// Eagerly decodes an object in a fresh session.
    ///
    /// References are resolved and remain active for the complete decoder call.
    /// A repeated active reference returns a cycle error.
    pub fn read<T: FromPdfObject>(&self, object: &ObjectVariant) -> ReadResult<T> {
        self.session().read(object)
    }

    /// Acquires and eagerly decodes an indirect object in a fresh session.
    pub fn read_indirect<T: FromPdfObject>(&self, id: ObjectId) -> ReadResult<T> {
        self.session().read_indirect(id)
    }

    /// Decodes through the typed cache, preserving recursive links as handles.
    ///
    /// For an indirect input, atomically reserve its typed entry before decoding.
    /// An existing loading entry returns a pending handle immediately, including
    /// recursive and concurrent requests. The first caller decodes synchronously.
    /// A direct input is decoded without deduplication into an owned ready handle.
    ///
    /// Failures are recorded for existing handles and returned to the caller.
    /// Pure reference-chain cycles still fail: they do not identify a value that
    /// can be decoded. No pending entry is synchronously waited upon.
    pub fn read_shared<T>(&self, object: &ObjectVariant) -> ReadResult<ObjectHandle<T>>
    where
        T: FromPdfObject + Send + Sync + 'static,
    {
        self.session().read_shared(object)
    }

    /// Reads an indirect object through the typed cache in a fresh session.
    ///
    /// The key uses the requested ID, including generation, and the Rust type.
    /// Different reference aliases are not required to share a typed entry.
    pub fn read_shared_indirect<T>(&self, id: ObjectId) -> ReadResult<ObjectHandle<T>>
    where
        T: FromPdfObject + Send + Sync + 'static,
    {
        self.session().read_shared_indirect(id)
    }
}

/// Provides nested reads to object contexts without exposing reader internals.
///
/// Generic methods deliberately use static dispatch. Implementations must keep
/// reference IDs active across nested decoder calls and restore traversal state
/// on every exit, including unwinding. They must not wait for typed cache entries.
///
/// Custom implementations can support alternate reading strategies or tests
/// without changing domain decoders.
pub trait ObjectAccess {
    /// Borrows the raw source for leaf parsers and bootstrap operations.
    fn source(&self) -> &dyn crate::object_resolver::ObjectResolver;

    /// Returns the content-stream ID allocator shared by this traversal.
    ///
    /// Implementations must return the same allocator throughout nested reads.
    fn content_stream_ids(&self) -> &ContentStreamIdAllocator;

    /// Acquires a raw indirect object without recursively resolving its contents.
    fn load_object(&mut self, id: ObjectId) -> ReadResult<PdfObject>;

    /// Resolves a top-level reference chain with cycle and depth checks.
    ///
    /// This operation ends its tracking scope before returning. Recursive
    /// decoding must use the read operations, which retain the scope.
    fn resolve(&mut self, object: &ObjectVariant) -> ReadResult<ResolvedObject>;

    /// Resolves and eagerly decodes within the current active path.
    fn read<T: FromPdfObject>(&mut self, object: &ObjectVariant) -> ReadResult<T>;

    /// Acquires and eagerly decodes within the current active path.
    fn read_indirect<T: FromPdfObject>(&mut self, id: ObjectId) -> ReadResult<T>;

    /// Reads through typed caching without waiting on recursive or concurrent loads.
    fn read_shared<T>(&mut self, object: &ObjectVariant) -> ReadResult<ObjectHandle<T>>
    where
        T: FromPdfObject + Send + Sync + 'static;

    /// Reads an indirect typed entry while preserving the current traversal.
    fn read_shared_indirect<T>(&mut self, id: ObjectId) -> ReadResult<ObjectHandle<T>>
    where
        T: FromPdfObject + Send + Sync + 'static;
}

/// Borrows a reader and holds the active state of one logical traversal.
///
/// An eager decoder's reference path remains active until it returns. Typed
/// cache lookups are handled before eager cycle rejection, so recursive shared
/// reads can return an already-published pending handle. Sibling visits are not
/// cycles once their earlier traversal scope has ended.
///
/// A session cannot outlive the reader it borrows:
///
/// ```compile_fail
/// use pdf_object_reader::{ObjectReader, ObjectSource, ReadSession};
///
/// fn escape<S: ObjectSource>(reader: &ObjectReader<S>) -> ReadSession<'static, S> {
///     reader.session()
/// }
/// ```
pub struct ReadSession<'reader, S, C = ConcurrentObjectCache> {
    reader: &'reader ObjectReader<S, C>,
    active_path: Vec<ObjectId>,
    active_ids: HashSet<ObjectId>,
    decode_depth: usize,
}

impl<S, C> ReadSession<'_, S, C> {
    /// Returns the current indirect-object path for diagnostics.
    pub fn active_path(&self) -> &[ObjectId] {
        &self.active_path
    }
}

impl<S, C> ObjectAccess for ReadSession<'_, S, C>
where
    S: ObjectSource,
    C: ObjectCache,
{
    fn source(&self) -> &dyn crate::object_resolver::ObjectResolver {
        &self.reader.source
    }

    fn content_stream_ids(&self) -> &ContentStreamIdAllocator {
        self.reader.content_stream_ids()
    }

    fn load_object(&mut self, id: ObjectId) -> ReadResult<PdfObject> {
        if let Some(object) =
            self.reader
                .raw_cache
                .get(id)
                .map_err(|source| ObjectReadError::Cache {
                    object_id: id,
                    source: Box::new(source),
                })?
        {
            return Ok(object);
        }
        let object = self
            .reader
            .source
            .read_object(id)
            .map_err(|source| ObjectReadError::Source {
                object_id: id,
                source: Box::new(source),
            })?
            .ok_or(ObjectReadError::MissingObject { object_id: id })?;
        self.reader
            .raw_cache
            .insert_if_absent(id, object)
            .map_err(|source| ObjectReadError::Cache {
                object_id: id,
                source: Box::new(source),
            })
    }

    fn resolve(&mut self, object: &ObjectVariant) -> ReadResult<ResolvedObject> {
        // Raw probes have chain-local tracking, so shared recursive edges can be inspected.
        let mut path = Vec::new();
        let mut ids = HashSet::new();
        let mut object = PdfObject::new(object.clone());
        while let ObjectVariant::Reference(id) = object.value() {
            let id = *id;
            if !ids.insert(id) {
                return Err(ObjectReadError::CyclicReference { repeated: id, path });
            }
            if path.len() >= self.reader.limits.max_reference_depth().get() {
                return Err(ObjectReadError::ReferenceDepthExceeded {
                    maximum: self.reader.limits.max_reference_depth().get(),
                    path,
                });
            }
            path.push(id);
            object = self.load_object(id)?;
        }
        ResolvedObject::try_from(object)
    }

    fn read<T: FromPdfObject>(&mut self, object: &ObjectVariant) -> ReadResult<T> {
        if self.decode_depth >= self.reader.limits.max_decode_depth().get() {
            return Err(ObjectReadError::DecodeDepthExceeded {
                maximum: self.reader.limits.max_decode_depth().get(),
            });
        }
        // The child owns its traversal scope. Parent state remains intact on errors and unwinding.
        let mut child = ReadSession {
            reader: self.reader,
            active_path: self.active_path.clone(),
            active_ids: self.active_ids.clone(),
            decode_depth: self.decode_depth.saturating_add(1),
        };
        let mut object = PdfObject::new(object.clone());
        while let ObjectVariant::Reference(id) = object.value() {
            let id = *id;
            if !child.active_ids.insert(id) {
                return Err(ObjectReadError::CyclicReference {
                    repeated: id,
                    path: child.active_path,
                });
            }
            if child.active_path.len() >= child.reader.limits.max_reference_depth().get() {
                return Err(ObjectReadError::ReferenceDepthExceeded {
                    maximum: child.reader.limits.max_reference_depth().get(),
                    path: child.active_path,
                });
            }
            child.active_path.push(id);
            object = child.load_object(id)?;
        }
        T::from_pdf_object(ObjectContext::new(
            ResolvedObject::try_from(object)?,
            &mut child,
        ))
    }

    fn read_indirect<T: FromPdfObject>(&mut self, id: ObjectId) -> ReadResult<T> {
        self.read(&ObjectVariant::Reference(id))
    }

    fn read_shared<T>(&mut self, object: &ObjectVariant) -> ReadResult<ObjectHandle<T>>
    where
        T: FromPdfObject + Send + Sync + 'static,
    {
        let ObjectVariant::Reference(id) = object else {
            return self
                .read(object)
                .map(|value| ObjectHandle::direct(Arc::new(value)));
        };
        let id = *id;
        match self.reader.typed_cache.reserve::<T>(id)? {
            Reservation::Existing(handle) => Ok(handle),
            Reservation::Vacant(reservation) => match self.read(object) {
                Ok(value) => reservation.publish(Arc::new(value)),
                Err(error) => {
                    let source = Arc::new(error);
                    reservation.fail(Arc::clone(&source))?;
                    Err(ObjectReadError::CachedDecode {
                        object_id: id,
                        target: std::any::type_name::<T>(),
                        source,
                    })
                }
            },
        }
    }

    fn read_shared_indirect<T>(&mut self, id: ObjectId) -> ReadResult<ObjectHandle<T>>
    where
        T: FromPdfObject + Send + Sync + 'static,
    {
        self.read_shared(&ObjectVariant::Reference(id))
    }
}
