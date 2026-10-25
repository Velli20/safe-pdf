//! Abstractions over storage and parsing of individual indirect objects.

use std::error::Error;

use crate::object_id::ObjectId;
use crate::pdf_object::PdfObject;

/// Acquires raw indirect objects for the resolution layer.
///
/// Implementations may parse a byte slice, read a memory-mapped file, query a
/// remote range source, or delegate to another object repository. Calls may be
/// made concurrently, so implementations must not rely on request-local state.
///
/// One source represents an immutable document snapshot. Repeated requests for
/// the same object and generation must have the same meaning. Implementations
/// must tolerate duplicate concurrent loads, because the raw cache stores only
/// completed objects. Sources must not call back into the reader while acquiring
/// a raw object.
pub trait ObjectSource: Send + Sync {
    /// The implementation-specific acquisition error.
    type Error: Error + Send + Sync + 'static;

    /// Reads one indirect object without recursively resolving its references.
    ///
    /// `Ok(None)` indicates that the identifier is absent, which the reader resolves
    /// to null (ISO 32000 §7.3.10). Return an error instead where absence must fail
    /// the read, such as an object that has not been loaded yet. Returned objects may
    /// themselves be references and may contain references at any depth.
    fn read_object(&self, object_id: ObjectId) -> Result<Option<PdfObject>, Self::Error>;
}

impl<S: ObjectSource + ?Sized> ObjectSource for &S {
    type Error = S::Error;
    fn read_object(&self, id: ObjectId) -> Result<Option<PdfObject>, Self::Error> {
        (**self).read_object(id)
    }
}

/// A source that holds no indirect objects.
///
/// Every reference read through it is absent and therefore reads as null (ISO 32000
/// §7.3.10), so only direct values decode. Use it where the input is known to contain no
/// indirect references, such as content-stream operands.
pub struct EmptySource;

impl ObjectSource for EmptySource {
    type Error = std::convert::Infallible;
    fn read_object(&self, _id: ObjectId) -> Result<Option<PdfObject>, Self::Error> {
        Ok(None)
    }
}
