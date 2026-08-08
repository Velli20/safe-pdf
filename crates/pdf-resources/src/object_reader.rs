//! Page-related PDF object reading with shared cycle protection.
//!
//! This module centralizes active-read cycle tracking used when parsing indirect
//! objects that may recursively reference each other.

use crate::{error::PdfPagesError, resource_cache::ResourceCache};
use pdf_content_stream::ContentStreamIdAllocator;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};
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
    pub(crate) fn begin_read(&mut self, obj_num: usize) -> bool {
        self.in_progress.insert(obj_num)
    }

    /// Clears the in-progress marker for the object.
    pub(crate) fn end_read(&mut self, obj_num: usize) {
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
    ) -> Result<Option<Self::Output>, PdfPagesError> {
        let Some(obj_num) = dictionary.object_number else {
            return Self::read_dictionary_inner(
                dictionary,
                objects,
                cache,
                cycle_tracker,
                id_allocator,
            )
            .map(Some);
        };

        if !cycle_tracker.begin_read(obj_num) {
            return Ok(None);
        }

        let result =
            Self::read_dictionary_inner(dictionary, objects, cache, cycle_tracker, id_allocator);
        cycle_tracker.end_read(obj_num);
        result.map(Some)
    }
}
