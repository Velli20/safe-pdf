use pdf_content_stream::content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_graphics::rect::Rect;
use pdf_graphics::transform::Transform;
use pdf_object::stream::StreamObject;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

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
        dictionary: &Dictionary,
        stream_data: &StreamObject,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfPagesError> {
        // Retrieve the `/BBox` entry.
        let bbox = Rect::from(
            dictionary
                .get_or_err("BBox")?
                .try_array_of::<f32, 4>(objects)?,
        );

        // Retrieve the `/Matrix` entry if present.
        let matrix = Matrix::from_dictionary(dictionary, objects)?;

        // Parse the `/Resources` entry if present, mapping any errors.
        let resources = Resources::read(dictionary, objects, cache, id_allocator)?;

        // Parse the content stream data.
        let content_stream = ContentStream::from_stream(stream_data, id_allocator)?;

        Ok(FormXObject {
            bbox,
            matrix,
            resources,
            content_stream,
        })
    }
}
