use std::rc::Rc;

use pdf_content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_graphics::rect::Rect;
use pdf_graphics::transform::Transform;
use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::error::PdfPagesError;
use crate::object_reader::ReadCycleTracker;
use crate::resource_cache::ResourceCache;
use crate::resources::Resources;

/// Represents a PDF Form XObject.
pub struct FormXObject {
    /// The bounding box of the form.
    pub bbox: Rect,
    /// Optional transformation matrix.
    pub matrix: Option<Transform>,
    /// Resources used by the form.
    pub resources: Option<Rc<Resources>>,
    /// The content stream that defines the graphics of the pattern cell.
    pub content_stream: ContentStream,
}

impl FormXObject {
    /// Builds a form XObject from parsed dictionary fields and a prepared content stream.
    ///
    /// This helper centralizes the shared `/BBox`, `/Matrix`, and `/Resources`
    /// parsing used by both stream-backed forms and dictionary-only fallback forms.
    fn from_dictionary_parts(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
        content_stream: ContentStream,
    ) -> Result<Self, PdfPagesError> {
        // Retrieve the `/BBox` entry.
        let bbox = dictionary.required_bbox(objects)?.normalized();

        // Retrieve the `/Matrix` entry if present.
        let matrix = dictionary.optional_matrix(objects)?;

        // Parse the `/Resources` entry if present, mapping any errors.
        let resources = Resources::read(dictionary, objects, cache, cycle_tracker, id_allocator)?;

        Ok(FormXObject {
            bbox,
            matrix,
            resources,
            content_stream,
        })
    }

    /// Parses a Form XObject from its dictionary and stream data.
    pub fn read_xobject(
        content: &ObjectVariant,
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfPagesError> {
        let content_stream = ContentStream::new(content, objects, id_allocator)?;
        Self::from_dictionary_parts(
            dictionary,
            objects,
            cache,
            cycle_tracker,
            id_allocator,
            content_stream,
        )
    }

    /// Parses a dictionary-only Form XObject and treats it as an empty form.
    pub fn empty_from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfPagesError> {
        let content_stream = ContentStream {
            operators: Vec::new(),
            id: id_allocator.next_id()?,
        };
        Self::from_dictionary_parts(
            dictionary,
            objects,
            cache,
            cycle_tracker,
            id_allocator,
            content_stream,
        )
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_content_stream::ContentStreamIdAllocator;
    use pdf_graphics::transform::Transform;
    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    use crate::{object_reader::ReadCycleTracker, resource_cache::DefaultResourceCache};

    use super::FormXObject;

    #[test]
    fn read_xobject_normalizes_inverted_bbox() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                "BBox".to_string(),
                ObjectVariant::Array(vec![
                    ObjectVariant::Real(265.077),
                    ObjectVariant::Real(71.8304),
                    ObjectVariant::Real(301.321),
                    ObjectVariant::Real(43.3206),
                ]),
            ),
            ("Subtype".to_string(), ObjectVariant::Name(b"Form".to_vec())),
        ]));
        let stream = StreamObject::new(7, 0, Box::new(dictionary.clone()), Vec::new());
        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let form = FormXObject::read_xobject(
            &ObjectVariant::Stream(stream),
            &dictionary,
            &PassthroughResolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        )
        .expect("form xobject should parse");

        assert_eq!(form.bbox.left, 265.077);
        assert_eq!(form.bbox.top, 43.3206);
        assert_eq!(form.bbox.right, 301.321);
        assert_eq!(form.bbox.bottom, 71.8304);
    }

    #[test]
    fn empty_from_dictionary_creates_empty_content_stream() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                "BBox".to_string(),
                ObjectVariant::Array(vec![
                    ObjectVariant::Real(0.0),
                    ObjectVariant::Real(0.0),
                    ObjectVariant::Real(10.0),
                    ObjectVariant::Real(10.0),
                ]),
            ),
            ("Subtype".to_string(), ObjectVariant::Name(b"Form".to_vec())),
            (
                "Matrix".to_string(),
                ObjectVariant::Array(vec![
                    ObjectVariant::Real(2.0),
                    ObjectVariant::Real(0.0),
                    ObjectVariant::Real(0.0),
                    ObjectVariant::Real(3.0),
                    ObjectVariant::Real(4.0),
                    ObjectVariant::Real(5.0),
                ]),
            ),
        ]));
        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let form = FormXObject::empty_from_dictionary(
            &dictionary,
            &PassthroughResolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        )
        .expect("dictionary-only form should parse");

        assert_eq!(form.bbox.left, 0.0);
        assert_eq!(form.bbox.top, 0.0);
        assert_eq!(form.bbox.right, 10.0);
        assert_eq!(form.bbox.bottom, 10.0);
        assert_eq!(
            form.matrix,
            Some(Transform::from_row(2.0, 0.0, 0.0, 3.0, 4.0, 5.0))
        );
        assert_eq!(form.content_stream.id, 0);
        assert!(form.content_stream.operators.is_empty());
    }
}
