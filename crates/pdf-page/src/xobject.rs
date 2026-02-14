use crate::{
    form::{FormXObject, FormXObjectError},
    image::{ImageXObject, ImageXObjectError},
    resource_cache::ResourceCache,
};
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    stream::StreamObject,
};
use thiserror::Error;

/// Represents a PDF External Object (XObject).
///
/// XObjects are reusable resources within a PDF file. They can be images,
/// self-contained graphical forms, or other types of external content.
#[allow(clippy::large_enum_variant)]
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

impl XObject {
    pub fn read_xobject(
        dictionary: &Dictionary,
        stream_data: &StreamObject,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
    ) -> Result<Self, XObjectError> {
        let subtype = dictionary.get_or_err("Subtype")?.try_str(objects)?;

        match subtype.as_ref() {
            "Image" => {
                let image_xobject =
                    ImageXObject::read_xobject(dictionary, stream_data, objects, cache)?;
                Ok(XObject::Image(image_xobject))
            }
            "Form" => {
                let form_xobject =
                    FormXObject::read_xobject(dictionary, stream_data, objects, cache)?;
                Ok(XObject::Form(Box::new(form_xobject)))
            }
            other => Err(XObjectError::UnsupportedXObjectType {
                subtype: other.to_string(),
            }),
        }
    }
}
