//! Trait-based readers for page-related PDF objects with shared cycle protection.
//!
//! These traits centralize active-read cycle tracking
//! used when parsing indirect objects that may recursively reference each other.
//! Implementers provide an inner read method containing the actual parsing logic,
//! while the default entrypoint methods handle cycle tracking consistently.

use crate::{error::PdfPagesError, resource_cache::ResourceCache};
use pdf_content_stream::ContentStreamIdAllocator;
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant, stream::StreamObject,
};
use std::collections::HashSet;

/// Tracks objects currently being parsed by page-related object readers.
#[derive(Default)]
pub struct ReadCycleTracker {
    in_progress: HashSet<usize>,
}

impl ReadCycleTracker {
    /// Marks the object as currently being parsed.
    ///
    /// Returns `false` when the object is already in progress, indicating a cycle.
    fn begin_read(&mut self, obj_num: usize) -> bool {
        self.in_progress.insert(obj_num)
    }

    /// Clears the in-progress marker for the object.
    fn end_read(&mut self, obj_num: usize) {
        self.in_progress.remove(&obj_num);
    }
}

/// Reads a dictionary-backed object with cycle protection.
///
/// Implementers provide [`ReadFromDictionary::read_dictionary_inner`] with the
/// actual parsing logic. The default [`ReadFromDictionary::from_dictionary`]
/// wrapper marks the object number as in-progress before parsing and always
/// clears that marker before returning.
pub trait ReadFromDictionary {
    /// The value produced by the read.
    type Output;

    /// Handles a repeated read of an object that is already in progress.
    ///
    /// The default behavior reports a cyclic dependency error for the object
    /// number. Implementers can override this when cyclic re-entry should be
    /// tolerated and mapped to a different result.
    fn cyclic_read(obj_num: usize) -> Result<Self::Output, PdfPagesError> {
        Err(ObjectError::CyclicDependency { obj_num }.into())
    }

    /// Reads the object after cycle tracking has been started.
    ///
    /// Implementations should focus only on parsing. They should not call
    /// `begin_read` or `end_read` directly.
    fn read_dictionary_inner(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self::Output, PdfPagesError>;

    /// Reads a dictionary-backed object and wraps the read in begin/end cycle tracking.
    ///
    /// When `dictionary.object_number` is present, this method registers the
    /// object as in-progress, delegates to
    /// [`ReadFromDictionary::read_dictionary_inner`], and then clears the
    /// in-progress marker before returning. When the dictionary has no object
    /// number, the inner read runs directly without cycle tracking.
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self::Output, PdfPagesError> {
        let Some(obj_num) = dictionary.object_number else {
            return Self::read_dictionary_inner(
                dictionary,
                objects,
                cache,
                cycle_tracker,
                id_allocator,
            );
        };

        if !cycle_tracker.begin_read(obj_num) {
            return Self::cyclic_read(obj_num);
        }

        let result =
            Self::read_dictionary_inner(dictionary, objects, cache, cycle_tracker, id_allocator);
        cycle_tracker.end_read(obj_num);
        result
    }
}

/// Reads a stream-backed XObject with cycle protection.
///
/// Implementers provide [`ReadXObject::read_xobject_inner`] with the parsing
/// logic for the stream and dictionary pair. The default
/// [`ReadXObject::read_xobject`] wrapper handles cycle tracking using the
/// stream object's number.
pub trait ReadXObject {
    /// Handles a repeated read of an object that is already in progress.
    ///
    /// The default behavior reports a cyclic dependency error for the stream's
    /// object number. Implementers can override this when cyclic re-entry
    /// should be mapped to a different result.
    fn cyclic_read(obj_num: usize) -> Result<Self, PdfPagesError>
    where
        Self: Sized,
    {
        Err(ObjectError::CyclicDependency { obj_num }.into())
    }

    /// Reads the XObject after cycle tracking has been started.
    ///
    /// Implementations should contain only the parsing logic for the XObject
    /// and should not call `begin_read` or `end_read` directly.
    fn read_xobject_inner(
        content: &ObjectVariant,
        dictionary: &Dictionary,
        stream_data: &StreamObject,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfPagesError>
    where
        Self: Sized;

    /// Reads an XObject and wraps the read in begin/end cycle tracking.
    ///
    /// This method marks `stream_data.object_number` as in-progress, delegates
    /// to [`ReadXObject::read_xobject_inner`], and clears the in-progress marker
    /// before returning.
    fn read_xobject(
        content: &ObjectVariant,
        dictionary: &Dictionary,
        stream_data: &StreamObject,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfPagesError>
    where
        Self: Sized,
    {
        if !cycle_tracker.begin_read(stream_data.object_number) {
            return Self::cyclic_read(stream_data.object_number);
        }

        let result = Self::read_xobject_inner(
            content,
            dictionary,
            stream_data,
            objects,
            cache,
            cycle_tracker,
            id_allocator,
        );
        cycle_tracker.end_read(stream_data.object_number);
        result
    }
}
