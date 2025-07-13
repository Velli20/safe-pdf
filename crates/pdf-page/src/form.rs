use pdf_content_stream::error::PdfOperatorError;
use pdf_object::{
    dictionary::Dictionary, object_collection::ObjectCollection, traits::FromDictionary,
};
use thiserror::Error;

use crate::bbox::{BBox, BBoxReadError};
use crate::content_stream::ContentStream;
use crate::matrix::{Matrix, MatrixReadError};
use crate::resources::{Resources, ResourcesError};
use crate::xobject::XObjectReader;

/// Errors that can occur during parsing of a Form XObject.
#[derive(Debug, Error)]
pub enum FormXObjectError {
    #[error("Missing required entry '{entry_name}' in Form XObject dictionary")]
    MissingEntry { entry_name: &'static str },
    #[error("Invalid type for entry '{entry_name}': expected {expected_type}, found {found_type}")]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str,
    },
    #[error("Error parsing /Resources: {source}")]
    ResourcesError { source: Box<ResourcesError> },
    #[error("Error parsing /Matrix: {0}")]
    InvalidMatrix(String),
    #[error("Error parsing content stream: {0}")]
    ContentStreamError(#[from] PdfOperatorError),
    #[error("Error parsing BBox: {0}")]
    BBoxReadError(#[from] BBoxReadError),
    #[error("Error parsing BBox: {0}")]
    MatrixReadError(#[from] MatrixReadError),
}

/// Represents a PDF Form XObject.
pub struct FormXObject {
    /// The bounding box of the form.
    pub bbox: [f32; 4],
    /// Optional transformation matrix.
    pub matrix: Option<[f32; 6]>,
    /// Resources used by the form.
    pub resources: Option<Resources>,
    /// The content stream (operators).
    pub content_stream: ContentStream,
}

impl XObjectReader for FormXObject {
    type ErrorType = FormXObjectError;

    /// Parses a Form XObject from its dictionary and stream data.
    fn read_xobject(
        dictionary: &Dictionary,
        stream_data: &[u8],
        objects: &ObjectCollection,
    ) -> Result<Self, FormXObjectError> {
        // Retrieve the `/BBox` entry,.
        let bbox = BBox::from_dictionary(dictionary, objects)?
            .ok_or(FormXObjectError::MissingEntry { entry_name: "BBox" })?
            .0;

        // Retrieve the `/Matrix` entry if present.
        let matrix = Matrix::from_dictionary(dictionary, objects)?.map_or(None, |m| Some(m.0));

        // Parse the `/Resources` entry if present, mapping any errors.
        let resources = Resources::from_dictionary(dictionary, objects).map_err(|err| {
            FormXObjectError::ResourcesError {
                source: Box::new(err),
            }
        })?;

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
