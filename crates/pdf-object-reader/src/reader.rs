//! Reader orchestration and session-local traversal state.

use std::any::TypeId;
use std::collections::HashSet;
use std::num::NonZeroUsize;

use crate::ContentStreamIdAllocator;
use crate::cache::Reservation;
use crate::cache::{ConcurrentObjectCache, ObjectCache, TypedObjectCache};
use crate::context::{DictionaryContext, ObjectContext};
use crate::decode::FromPdfObject;
use crate::dictionary::Dictionary;
use crate::error::{ObjectReadError, ReadResult};
use crate::handle::ObjectHandle;
use crate::object_id::ObjectId;
use crate::object_variant::ObjectVariant;
use crate::pdf_object::PdfObject;
use crate::source::ObjectSource;
use std::sync::Arc;

/// Bounds nested decoding, including direct containers and reference hops.
///
/// The limit defaults to 128. It counts active stack depth, not total objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadLimits {
    max_decode_depth: NonZeroUsize,
}

impl ReadLimits {
    /// Configures the limit on simultaneously active decoder invocations.
    pub fn new(max_decode_depth: NonZeroUsize) -> Self {
        Self { max_decode_depth }
    }

    /// Returns the maximum nesting of decoders, including direct array elements.
    pub fn max_decode_depth(self) -> NonZeroUsize {
        self.max_decode_depth
    }
}

impl Default for ReadLimits {
    fn default() -> Self {
        Self::new(NonZeroUsize::new(128).unwrap_or(NonZeroUsize::MIN))
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

    /// Returns the decoding depth limit.
    pub fn limits(&self) -> ReadLimits {
        self.limits
    }

    /// Starts a fresh traversal while retaining this reader's caches and content IDs.
    pub fn session(&self) -> ReadSession<'_, S, C> {
        ReadSession {
            reader: self,
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

    /// Reads a required dictionary entry in a fresh session.
    ///
    /// A missing key and decoding failures are annotated with the entry's key.
    pub fn required<T: FromPdfObject>(&self, dictionary: &Dictionary, key: &[u8]) -> ReadResult<T> {
        DictionaryContext::new(dictionary, &mut self.session()).required(key)
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
    /// Returns the content-stream ID allocator shared by this traversal.
    ///
    /// Implementations must return the same allocator throughout nested reads.
    fn content_stream_ids(&self) -> &ContentStreamIdAllocator;

    /// Acquires a raw indirect object without recursively resolving its contents.
    fn load_object(&mut self, id: ObjectId) -> ReadResult<PdfObject>;

    /// Resolves a top-level reference chain with cycle and depth checks.
    ///
    /// The returned object is never a reference.
    ///
    /// This operation ends its tracking scope before returning. Recursive
    /// decoding must use the read operations, which retain the scope.
    fn resolve(&mut self, object: &ObjectVariant) -> ReadResult<PdfObject>;

    /// Resolves and eagerly decodes within the current traversal.
    fn read<T: FromPdfObject>(&mut self, object: &ObjectVariant) -> ReadResult<T>;

    /// Acquires and eagerly decodes within the current traversal.
    fn read_indirect<T: FromPdfObject>(&mut self, id: ObjectId) -> ReadResult<T>;

    /// Reads through typed caching without waiting on recursive or concurrent loads.
    fn read_shared<T>(&mut self, object: &ObjectVariant) -> ReadResult<ObjectHandle<T>>
    where
        T: FromPdfObject + Send + Sync + 'static;

    /// Reads an indirect typed entry while preserving the current traversal.
    fn read_shared_indirect<T>(&mut self, id: ObjectId) -> ReadResult<ObjectHandle<T>>
    where
        T: FromPdfObject + Send + Sync + 'static;

    /// Reads the object a decoder reached through `id` as a shared `T`, first ending the
    /// eager scope that decoder (cycle key `key`) holds on the reference chain from `id`.
    ///
    /// Recursion back to that object then meets the typed cache's pending handle rather
    /// than a cycle error. The decoder must not read anything further afterwards.
    fn read_shared_reached<T>(&mut self, id: ObjectId, key: TypeId) -> ReadResult<ObjectHandle<T>>
    where
        T: FromPdfObject + Send + Sync + 'static;
}

/// Borrows a reader and holds the active state of one logical traversal.
///
/// An object an eager decoder reached by reference stays active until it returns. A cycle is a
/// decoder re-entering an object it is still decoding, compared by
/// [`FromPdfObject::cycle_key`]; another decoder may read that object, as when a
/// membership dictionary names itself in its own visibility expression. Each decoder
/// decodes an object the same way every time, so only the same decoder re-entering it
/// could recurse forever. Typed
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
    /// The objects being decoded, each with the cycle key of the decoder reading it.
    active_ids: HashSet<(ObjectId, TypeId)>,
    /// The number of active decoder invocations, bounded by [`ReadLimits`].
    decode_depth: usize,
}

impl<S, C> ObjectAccess for ReadSession<'_, S, C>
where
    S: ObjectSource,
    C: ObjectCache,
{
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
            // A reference to a nonexistent object is null (ISO 32000 §7.3.10).
            .unwrap_or_else(|| PdfObject::new(ObjectVariant::Null));
        self.reader
            .raw_cache
            .insert_if_absent(id, object)
            .map_err(|source| ObjectReadError::Cache {
                object_id: id,
                source: Box::new(source),
            })
    }

    fn resolve(&mut self, object: &ObjectVariant) -> ReadResult<PdfObject> {
        // Raw probes have chain-local tracking, so shared recursive edges can be inspected.
        let mut ids = HashSet::new();
        let mut object = PdfObject::new(object.clone());
        while let ObjectVariant::Reference(id) = object.value() {
            let id = *id;
            if !ids.insert(id) {
                return Err(ObjectReadError::CyclicReference { repeated: id });
            }
            object = self.load_object(id)?;
        }
        Ok(object)
    }

    #[inline]
    fn read<T: FromPdfObject>(&mut self, object: &ObjectVariant) -> ReadResult<T> {
        if self.decode_depth >= self.reader.limits.max_decode_depth().get() {
            return Err(ObjectReadError::DecodeDepthExceeded {
                maximum: self.reader.limits.max_decode_depth().get(),
            });
        }
        let ObjectVariant::Reference(id) = object else {
            // A direct value adds nothing to the active path, so it decodes in this
            // session without copying the value or the traversal state.
            self.decode_depth = self.decode_depth.saturating_add(1);
            let result = T::from_pdf_object(ObjectContext::new(object, self));
            self.decode_depth = self.decode_depth.saturating_sub(1);
            return result;
        };
        self.read_reference(*id)
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

    fn read_shared_reached<T>(&mut self, id: ObjectId, key: TypeId) -> ReadResult<ObjectHandle<T>>
    where
        T: FromPdfObject + Send + Sync + 'static,
    {
        // Each link's mark is removed at most once, so the walk ends even on a malformed chain.
        let mut link = id;
        while self.active_ids.remove(&(link, key)) {
            match self.load_object(link)?.value() {
                ObjectVariant::Reference(next) => link = *next,
                _ => break,
            }
        }
        self.read_shared_indirect(id)
    }
}

impl<S, C> ReadSession<'_, S, C>
where
    S: ObjectSource,
    C: ObjectCache,
{
    /// Follows a reference chain and decodes its target in a child traversal scope.
    ///
    /// Kept out of [`ObjectAccess::read`] so the direct-value path stays small enough to inline.
    fn read_reference<T: FromPdfObject>(&mut self, requested: ObjectId) -> ReadResult<T> {
        let mut id = requested;
        // The child owns its traversal scope. Parent state remains intact on errors and unwinding.
        let mut child = ReadSession {
            reader: self.reader,
            active_ids: self.active_ids.clone(),
            decode_depth: self.decode_depth.saturating_add(1),
        };
        let object = loop {
            if !child.active_ids.insert((id, T::cycle_key())) {
                return Err(ObjectReadError::CyclicReference { repeated: id });
            }
            let object = child.load_object(id)?;
            match object.value() {
                ObjectVariant::Reference(next) => id = *next,
                _ => break object,
            }
        };
        T::from_pdf_object(ObjectContext::referenced(
            object.value(),
            (requested, T::cycle_key()),
            &mut child,
        ))
    }
}
