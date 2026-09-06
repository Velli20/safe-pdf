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
pub trait ObjectSource: crate::object_resolver::ObjectResolver + Send + Sync {
    /// The implementation-specific acquisition error.
    type Error: Error + Send + Sync + 'static;

    /// Reads one indirect object without recursively resolving its references.
    ///
    /// `Ok(None)` indicates that the identifier is absent. Returned objects may
    /// themselves be references and may contain references at any depth.
    fn read_object(&self, object_id: ObjectId) -> Result<Option<PdfObject>, Self::Error>;
}

impl<S: ObjectSource + ?Sized> ObjectSource for &S {
    type Error = S::Error;
    fn read_object(&self, id: ObjectId) -> Result<Option<PdfObject>, Self::Error> {
        (**self).read_object(id)
    }
}
