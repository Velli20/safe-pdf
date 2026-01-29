use pdf_content_stream::error::PdfOperatorError;
use pdf_graphics::rect::Rect;
use pdf_graphics::transform::Transform;
use pdf_object::error::ObjectError;
use pdf_object::stream::StreamObject;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver, traits::FromDictionary};
use thiserror::Error;

use crate::content_stream::ContentStream;
use crate::matrix::Matrix;
use crate::resources::{Resources, ResourcesError};
use crate::xobject::XObjectReader;

/// Errors that can occur during parsing of a Form XObject.
#[derive(Debug, Error)]
pub enum FormXObjectError {
    #[error("Error parsing /Resources: {source}")]
    ResourcesError { source: Box<ResourcesError> },
    #[error("Error parsing content stream: {0}")]
    ContentStreamError(#[from] PdfOperatorError),
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
}

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

impl XObjectReader for FormXObject {
    type ErrorType = FormXObjectError;

    /// Parses a Form XObject from its dictionary and stream data.
    fn read_xobject(
        dictionary: &Dictionary,
        stream_data: &StreamObject,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, FormXObjectError> {
        // Retrieve the `/BBox` entry.
        let bbox = Rect::from(
            dictionary
                .get_or_err("BBox")?
                .try_array_of::<f32, 4>(objects)?,
        );

        // Retrieve the `/Matrix` entry if present.
        let matrix = Matrix::from_dictionary(dictionary, objects)?;

        // Parse the `/Resources` entry if present, mapping any errors.
        let resources = Resources::from_dictionary(dictionary, objects).map_err(|err| {
            FormXObjectError::ResourcesError {
                source: Box::new(err),
            }
        })?;

        let stream_data = stream_data.data()?;
        // Parse the content stream data.
        let content_stream = ContentStream {
            operations: pdf_content_stream::pdf_operator::PdfOperatorVariant::from(&stream_data)?,
        };

        Ok(FormXObject {
            bbox,
            matrix,
            resources,
            content_stream,
        })
    }
}
