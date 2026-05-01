use crate::{error::PdfPagesError, form::FormXObject, resource_cache::ResourceCache};
use pdf_content_stream::content_stream::ContentStreamIdAllocator;
use pdf_image::{ImageXObject, PdfImageError, SoftMaskResolver};
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
        if !cache.begin_read(stream_data.object_number) {
            return Err(pdf_object::error::ObjectError::CyclicDependency {
                obj_num: stream_data.object_number,
            }
            .into());
        }

        let result =
            Self::read_xobject_inner(dictionary, stream_data, objects, cache, id_allocator);
        cache.end_read(stream_data.object_number);
        result
    }

    fn read_xobject_inner(
        dictionary: &Dictionary,
        stream_data: &StreamObject,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfPagesError> {
        let subtype = dictionary.get_or_err("Subtype")?.try_str(objects)?;

        match subtype.as_ref() {
            "Image" => {
                let mut soft_mask_resolver = PageSoftMaskResolver {
                    cache,
                    id_allocator,
                };
                let image_xobject = ImageXObject::read_xobject(
                    dictionary,
                    stream_data,
                    objects,
                    &mut soft_mask_resolver,
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

struct PageSoftMaskResolver<'a> {
    cache: &'a mut dyn ResourceCache,
    id_allocator: &'a mut ContentStreamIdAllocator,
}

impl SoftMaskResolver for PageSoftMaskResolver<'_> {
    fn resolve_soft_mask(
        &mut self,
        stream: &StreamObject,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<ImageXObject>, PdfImageError> {
        let cache = &mut *self.cache;
        let id_allocator = &mut *self.id_allocator;
        if !cache.begin_read(stream.object_number) {
            return Ok(None);
        }

        let result =
            match XObject::read_xobject(&stream.dictionary, stream, objects, cache, id_allocator) {
                Ok(XObject::Image(image)) => Ok(Some(image)),
                Ok(_) => Err(PdfImageError::InvalidSoftMaskXObject),
                Err(err) if err.is_cyclic_dependency() => Ok(None),
                Err(PdfPagesError::Image(err)) => Err(err),
                Err(PdfPagesError::Object(err)) => Err(err.into()),
                Err(PdfPagesError::ColorSpace(err)) => Err(err.into()),
                Err(_) => Err(PdfImageError::InvalidSoftMaskXObject),
            };

        cache.end_read(stream.object_number);
        result
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;
    use pdf_content_stream::content_stream::ContentStreamIdAllocator;
    use pdf_object::{
        dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
        stream::StreamObject,
    };

    use crate::resource_cache::DefaultResourceCache;

    use super::XObject;

    struct SelfReferentialResolver {
        stream: ObjectVariant,
    }

    impl ObjectResolver for SelfReferentialResolver {
        fn resolve_object<'a>(
            &'a self,
            obj: &'a ObjectVariant,
        ) -> Result<&'a ObjectVariant, pdf_object::error::ObjectError> {
            match obj {
                ObjectVariant::Reference(_) => Ok(&self.stream),
                _ => Ok(obj),
            }
        }
    }

    fn image_dictionary(object_number: usize) -> Dictionary {
        Dictionary::new(BTreeMap::from([
            (
                "Subtype".to_string(),
                ObjectVariant::Name(b"Image".to_vec()),
            ),
            ("Width".to_string(), ObjectVariant::Integer(1)),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            ("SMask".to_string(), ObjectVariant::Reference(object_number)),
        ]))
    }

    #[test]
    fn self_referential_soft_mask_is_treated_as_absent() {
        let stream = StreamObject::new(7, 0, Box::new(image_dictionary(7)), vec![0xAA]);
        let resolver = SelfReferentialResolver {
            stream: ObjectVariant::Stream(stream.clone()),
        };
        let mut cache = DefaultResourceCache::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let xobject = XObject::read_xobject(
            &stream.dictionary,
            &stream,
            &resolver,
            &mut cache,
            &mut id_allocator,
        )
        .expect("self-referential soft masks should not fail image parsing");

        assert!(matches!(&xobject, XObject::Image(_)));
        if let XObject::Image(image) = xobject {
            assert_eq!(image.width, 1);
            assert_eq!(image.height, 1);
            assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::Gray8);
            assert_eq!(image.data, vec![0xAA]);
        }
    }
}
