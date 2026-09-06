//! PDF object parsing, scoped decoding, and reader-owned typed resources.
//!
//! # Reading model
//!
//! An [`ObjectReader`] owns one immutable document snapshot, raw and typed
//! caches, and content-stream IDs. Each [`ReadSession`] owns an independent active
//! traversal. Domain types implement [`FromPdfObject`] and receive one
//! [`ObjectContext`], which combines the current value with nested reads and
//! content-stream IDs. Dictionary, array, and stream contexts preserve this
//! traversal.
//!
//! Eager reads reject recursive references. Shared reads reserve a typed entry
//! before decoding and preserve recursive links through [`ObjectHandle`].
//! Concurrent requests can return pending handles; they never wait for another
//! decoder. The reader owns indirect values, and graph edges use weak handles.
//! Source and decoder code must never execute while a cache lock is held.
//!
//! # Content-stream IDs and dictionary decoding
//!
//! Every reader owns an atomic allocator shared across its sessions. This
//! example uses the allocator from a one-context decoder.
//!
//! ```no_run
//! use pdf_object_reader::{
//!     FromPdfObject, ObjectAccess, ObjectContext, object_id::ObjectId, ObjectReader,
//!     ObjectSource, ReadResult,
//! };
//!
//! struct PageInfo {
//!     rotation: f64,
//!     content_id: usize,
//! }
//!
//! impl FromPdfObject for PageInfo {
//!     fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self>
//!     {
//!         let mut dictionary = context.dictionary()?;
//!         let rotation = dictionary.optional(b"Rotate")?.unwrap_or(0.0);
//!         let content_id = dictionary.content_stream_ids().next_id().map_err(|source| {
//!             pdf_object_reader::ObjectReadError::Decode {
//!                 target: "PageInfo", source: Box::new(source),
//!             }
//!         })?;
//!         Ok(Self { rotation, content_id })
//!     }
//! }
//!
//! fn read_page<S: ObjectSource>(source: S) -> ReadResult<PageInfo> {
//!     let reader = ObjectReader::new(source);
//!     reader.read_indirect(ObjectId::new(1, 0))
//! }
//! ```
//!
//! # Nested values and stream metadata
//!
//! ```no_run
//! use pdf_object_reader::{ObjectAccess, ObjectContext, StreamObject, ReadResult};
//!
//! fn inspect(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<()> {
//!     let mut dictionary = context.dictionary()?;
//!     let _count: f64 = dictionary.required(b"Count")?;
//!     let _rotation: Option<f64> = dictionary.optional(b"Rotate")?;
//!     let _bounds: Vec<f64> = dictionary.required(b"MediaBox")?;
//!     let stream: StreamObject = dictionary.required(b"Contents")?;
//!     let _data = stream.raw_data();
//!     let _filter_state = stream.filters_applied();
//!     Ok(())
//! }
//!
//! fn inspect_stream(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<()> {
//!     let mut stream = context.stream()?;
//!     let _length: f64 = stream.dictionary().required(b"Length")?;
//!     let _bytes = stream.stream().raw_data();
//!     Ok(())
//! }
//!
//! fn inspect_array(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Vec<f64>> {
//!     let mut array = context.array()?;
//!     let _first: f64 = array.at(0)?;
//!     array.read_all()
//! }
//! ```
//!
//! # Recursive resources
//!
//! The generic handle replaces resource-specific placeholder variants and
//! `LazyCacheValue` implementations. `read_shared` performs the coordination
//! previously performed by `read_resource_lazy`.
//!
//! ```no_run
//! use pdf_object_reader::{
//!     FromPdfObject, ObjectAccess, ObjectContext, ObjectHandle,
//!     object_id::ObjectId, ObjectReader, ObjectSource, ReadResult,
//! };
//!
//! struct Resource {
//!     parent: Option<ObjectHandle<Resource>>,
//! }
//!
//! impl FromPdfObject for Resource {
//!     fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self>
//!     {
//!         Ok(Self { parent: context.dictionary()?.optional_shared(b"Parent")? })
//!     }
//! }
//!
//! fn resource_edges(
//!     context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
//! ) -> ReadResult<Option<ObjectHandle<Resource>>> {
//!     context.dictionary()?.optional_shared(b"Parent")
//! }
//!
//! fn read_resource<S: ObjectSource>(source: S) -> ReadResult<()> {
//!     let reader = ObjectReader::new(source);
//!     let resource = reader.read_shared_indirect::<Resource>(ObjectId::new(7, 0))?;
//!     let _phase = resource.state()?;
//!     let _value_if_ready = resource.try_get()?;
//!     // Keep reader alive while following indirect graph edges.
//!     Ok(())
//! }
//! ```
//!
//! # Extension and migration boundaries
//!
//! [`ObjectSource`] and [`ObjectCache`] accept custom storage strategies;
//! [`ObjectAccess`] supports custom traversal implementations. A custom access
//! implementation can validate a direct value with
//! `ResolvedObject::try_from(object)`, then construct an [`ObjectContext`].
//! It remains responsible for maintaining the traversal scope around the decoder.
//!
//! The parser and domain decoders use the same [`ObjectVariant`] model.
//! Indirect references preserve both the object number and generation. Raw
//! source access remains available for parsing, filters, and encryption bootstrap;
//! recursive domain reads use contexts so their reference scopes remain active.
//!
//! Cached values and failures belong to one immutable snapshot. Direct inputs
//! bypass typed deduplication; pass references to preserve indirect identity.
//! Keep the reader alive while following indirect handles. Page owners in
//! `pdf-document` retain their reader, including when detached from a document.

/// Completed raw caching and internal typed-cache coordination.
pub mod cache;
mod content_stream_id_allocator;
/// Object-bound interfaces for reading typed child values.
pub mod context;
/// The one-context client decoding trait and built-in conversions.
pub mod decode;
/// Structured acquisition, traversal, decoding, and handle errors.
pub mod error;
/// Typed deferred handles for reader-owned resource graphs.
pub mod handle;
/// Indirect PDF object identifiers.
pub mod object_id;
/// Runtime classifications of PDF objects.
pub mod object_kind;
/// PDF object variants and checked accessors.
pub mod object_variant;
/// Immutable PDF arrays.
pub mod pdf_array;
/// Immutable PDF names.
pub mod pdf_name;
/// Immutable shared PDF object handles.
pub mod pdf_object;
/// Immutable PDF strings.
pub mod pdf_string;
/// Reader configuration and session-local traversal contracts.
pub mod reader;
/// Checked direct PDF object handles.
pub mod resolved_object;
/// Acquisition of raw indirect objects from a document snapshot.
pub mod source;
/// PDF string source representations.
pub mod string_kind;

pub use cache::{ConcurrentObjectCache, ConcurrentObjectCacheError, ObjectCache};
pub use content_stream_id_allocator::{ContentStreamIdAllocator, ContentStreamIdExhausted};
pub use context::{ArrayContext, DictionaryContext, ObjectContext, StreamContext};
pub use decode::FromPdfObject;
pub use error::{BoxedError, ObjectReadError, ReadLocation, ReadResult};
pub use handle::{HandleState, ObjectHandle};
pub use reader::{ObjectAccess, ObjectReader, ReadLimits, ReadSession};
pub use source::ObjectSource;

/// PDF cross reference table model and parsing operations.
pub mod cross_reference_table;
/// PDF dictionary model and parsing operations.
pub mod dictionary;
/// PDF object error model and parsing operations.
pub mod object_error;
/// PDF object lookup model and parsing operations.
pub mod object_lookup;
/// PDF object resolver model and parsing operations.
pub mod object_resolver;
/// PDF stream model and parsing operations.
pub mod stream;
/// PDF text encoding model and parsing operations.
pub mod text_encoding;
/// PDF trailer model and parsing operations.
pub mod trailer;
/// PDF version model and parsing operations.
pub mod version;

pub use dictionary::Dictionary;
pub use stream::StreamObject;
