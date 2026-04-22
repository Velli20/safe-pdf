use crate::{
    error::PdfPagesError, form::FormXObject, image::ImageXObject, resource_cache::ResourceCache,
};
use pdf_content_stream::content_stream::ContentStreamIdAllocator;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver, stream::StreamObject};

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

impl XObject {
    pub fn read_xobject(
        dictionary: &Dictionary,
        stream_data: &StreamObject,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfPagesError> {
        let subtype = dictionary.get_or_err("Subtype")?.try_str(objects)?;

        match subtype.as_ref() {
            "Image" => {
                let image_xobject = ImageXObject::read_xobject(
                    dictionary,
                    stream_data,
                    objects,
                    cache,
                    id_allocator,
                )?;
                Ok(XObject::Image(image_xobject))
            }
            "Form" => {
                let form_xobject = FormXObject::read_xobject(
                    dictionary,
                    stream_data,
                    objects,
                    cache,
                    id_allocator,
                )?;
                Ok(XObject::Form(Box::new(form_xobject)))
            }
            other => Err(PdfPagesError::UnsupportedXObjectSubtype {
                subtype: other.to_string(),
            }),
        }
    }
}
