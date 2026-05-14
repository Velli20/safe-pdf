use pdf_content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_graphics::rect::Rect;
use pdf_graphics::transform::Transform;
use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::error::PdfPagesError;
use crate::matrix::Matrix;
use crate::resource_cache::ResourceCache;
use crate::resources::Resources;

/// Represents a PDF Form XObject.
pub struct FormXObject {
    /// The bounding box of the form.
    pub bbox: Rect,
    /// Optional transformation matrix.
    pub matrix: Option<Transform>,
    /// Resources used by the form.
    pub resources: Option<Resources>,
    /// The content stream that defines the graphics of the pattern cell.
    pub content_stream: ContentStream,
}

impl FormXObject {
    /// Parses a Form XObject from its dictionary and stream data.
    pub fn read_xobject(
        content: &ObjectVariant,
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfPagesError> {
        // Retrieve the `/BBox` entry.
        let bbox = Rect::from(
            dictionary
                .get_or_err("BBox")?
                .try_array_of::<f32, 4>(objects)?,
        )
        .normalized();

        // Retrieve the `/Matrix` entry if present.
        let matrix = Matrix::from_dictionary(dictionary, objects)?;

        // Parse the `/Resources` entry if present, mapping any errors.
        let resources = Resources::read(dictionary, objects, cache, id_allocator)?;

        // Parse the content stream data.
        let content_stream = ContentStream::new(content, objects, id_allocator)?;

        Ok(FormXObject {
            bbox,
            matrix,
            resources,
            content_stream,
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_content_stream::ContentStreamIdAllocator;
    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    use crate::resource_cache::DefaultResourceCache;

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
        let mut id_allocator = ContentStreamIdAllocator::new();

        let form = FormXObject::read_xobject(
            &ObjectVariant::Stream(stream),
            &dictionary,
            &PassthroughResolver,
            &mut cache,
            &mut id_allocator,
        )
        .expect("form xobject should parse");

        assert_eq!(form.bbox.left, 265.077);
        assert_eq!(form.bbox.top, 43.3206);
        assert_eq!(form.bbox.right, 301.321);
        assert_eq!(form.bbox.bottom, 71.8304);
    }
}
