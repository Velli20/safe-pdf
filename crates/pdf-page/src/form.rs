use pdf_content_stream::error::PdfOperatorError;
use pdf_object::{
    dictionary::Dictionary, object_collection::ObjectCollection, traits::FromDictionary,
};
use thiserror::Error;

use crate::content_stream::ContentStream;
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

    #[error("Error parsing /FormType: {0}")]
    InvalidFormType(String),
    #[error("Error parsing /Matrix: {0}")]
    InvalidMatrix(String),
    #[error("Error parsing content stream: {0}")]
    ContentStreamError(#[from] PdfOperatorError),
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
        // /FormType (required, should be 1)
        let form_type = dictionary.get_number("FormType").unwrap_or(1) as i32;
        if form_type != 1 {
            return Err(FormXObjectError::InvalidFormType(form_type.to_string()));
        }

        // /BBox (required)
        let bbox = if let Some(matrix_obj) = dictionary.get("BBox") {
            let arr = matrix_obj
                .as_array()
                .ok_or(FormXObjectError::InvalidEntryType {
                    entry_name: "BBox",
                    expected_type: "Array",
                    found_type: matrix_obj.name(),
                })?;
            if arr.len() != 4 {
                return Err(FormXObjectError::InvalidMatrix(format!(
                    "Expected 4 elements, got {}",
                    arr.len()
                )));
            }
            let mut vals = [0.0f32; 4];
            for (i, obj) in arr.iter().enumerate() {
                vals[i] = obj.as_number::<f32>().map_err(|_| {
                    FormXObjectError::InvalidMatrix(format!(
                        "Element {} is not a number (found {})",
                        i,
                        obj.name()
                    ))
                })?;
            }
            vals
        } else {
            return Err(FormXObjectError::MissingEntry { entry_name: "BBox" });
        };

        // /Matrix (optional)
        let matrix = if let Some(matrix_obj) = dictionary.get("Matrix") {
            let arr = matrix_obj
                .as_array()
                .ok_or(FormXObjectError::InvalidEntryType {
                    entry_name: "Matrix",
                    expected_type: "Array",
                    found_type: matrix_obj.name(),
                })?;
            if arr.len() != 6 {
                return Err(FormXObjectError::InvalidMatrix(format!(
                    "Expected 6 elements, got {}",
                    arr.len()
                )));
            }
            let mut vals = [0.0f32; 6];
            for (i, obj) in arr.iter().enumerate() {
                vals[i] = obj.as_number::<f32>().map_err(|_| {
                    FormXObjectError::InvalidMatrix(format!(
                        "Element {} is not a number (found {})",
                        i,
                        obj.name()
                    ))
                })?;
            }
            Some(vals)
        } else {
            None
        };

        // /Resources (optional)
        let resources = Resources::from_dictionary(dictionary, objects).map_err(|err| {
            FormXObjectError::ResourcesError {
                source: Box::new(err),
            }
        })?;

        // Content stream
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
