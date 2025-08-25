use crate::{
    form::{FormXObject, FormXObjectError},
    image::{ImageXObject, ImageXObjectError},
};
use pdf_object::{dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection};
use thiserror::Error;

/// Represents a PDF External Object (XObject).
///
/// XObjects are reusable resources within a PDF file. They can be images,
/// self-contained graphical forms, or other types of external content.
pub enum XObject {
    /// An image XObject, representing a raster image.
    Image(ImageXObject),
    /// A form XObject, which is a self-contained sequence of graphics objects
    /// that can be painted as a single unit.
    Form(Box<FormXObject>),
}

#[derive(Debug, Error)]
pub enum XObjectError {
    #[error("Error parsing Image XObject: {0}")]
    ImageReadError(#[from] ImageXObjectError),
    #[error("Error parsing Form XObject: {0}")]
    FormReadError(#[from] FormXObjectError),
    #[error("Unsupported XObject type: '{subtype}'")]
    UnsupportedXObjectType { subtype: String },
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
}

/// A trait for parsing specific types of XObjects from their dictionary and stream data.
///
/// This internal trait provides a common interface for different XObject parsers
/// (like `ImageXObject` or `FormXObject`) to be constructed from the raw components
/// of a PDF stream object.
pub(crate) trait XObjectReader {
    type ErrorType;

    /// Parses an XObject from its dictionary and associated stream data.
    ///
    /// # Parameters
    ///
    /// - `dictionary`: The dictionary part of the XObject stream.
    /// - `stream_data`: The raw byte data of the XObject stream.
    /// - `objects`: A collection of all PDF objects in the document, used to resolve
    ///   any indirect references within the XObject's dictionary.
    ///
    /// # Returns
    ///
    /// A `Result` containing the parsed XObject of type `Self` on success,
    /// or an error of type `Self::ErrorType` on failure.
    fn read_xobject(
        dictionary: &Dictionary,
        stream_data: &[u8],
        objects: &ObjectCollection,
    ) -> Result<Self, Self::ErrorType>
    where
        Self: Sized;
}

impl XObjectReader for XObject {
    type ErrorType = XObjectError;

    fn read_xobject(
        dictionary: &Dictionary,
        stream_data: &[u8],
        objects: &ObjectCollection,
    ) -> Result<Self, Self::ErrorType> {
        let subtype = dictionary.get_or_err("Subtype")?.try_str()?;

        match subtype.as_ref() {
            "Image" => {
                let image_xobject = ImageXObject::read_xobject(dictionary, stream_data, objects)?;
                Ok(XObject::Image(image_xobject))
            }
            "Form" => {
                let form_xobject = FormXObject::read_xobject(dictionary, stream_data, objects)?;
                Ok(XObject::Form(Box::new(form_xobject)))
            }
            other => Err(XObjectError::UnsupportedXObjectType {
                subtype: other.to_string(),
            }),
        }
    }
}
