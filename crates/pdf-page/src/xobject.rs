use crate::{error::PdfPagesError, form::FormXObject, resource_cache::ResourceCache};
use pdf_content_stream::ContentStreamIdAllocator;
use pdf_image::{ImageXObject, PdfImageError, SoftMaskResolver};
use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
    stream::StreamObject,
};

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
        content: &ObjectVariant,
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

        let result = Self::read_xobject_inner(
            content,
            dictionary,
            stream_data,
            objects,
            cache,
            id_allocator,
        );
        cache.end_read(stream_data.object_number);
        result
    }

    fn read_xobject_inner(
        content: &ObjectVariant,
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
                let form_xobject =
                    FormXObject::read_xobject(content, dictionary, objects, cache, id_allocator)?;
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

        match XObject::read_xobject(
            &ObjectVariant::Stream(stream.clone()),
            &stream.dictionary,
            stream,
            objects,
            cache,
            id_allocator,
        ) {
            Ok(XObject::Image(image)) => Ok(Some(image)),
            Ok(_) => Err(PdfImageError::InvalidSoftMaskXObject),
            Err(err) if err.is_cyclic_dependency() => Ok(None),
            Err(PdfPagesError::Image(err)) => Err(err),
            Err(PdfPagesError::Object(err)) => Err(err.into()),
            Err(PdfPagesError::ColorSpace(err)) => Err(err.into()),
            Err(_) => Err(PdfImageError::InvalidSoftMaskXObject),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use pdf_content_stream::ContentStreamIdAllocator;
    use pdf_object::{
        dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };
    use std::collections::BTreeMap;

    use crate::resource_cache::DefaultResourceCache;

    use super::XObject;

    struct MapResolver {
        objects: BTreeMap<usize, ObjectVariant>,
    }

    impl ObjectResolver for MapResolver {
        fn resolve_object<'a>(
            &'a self,
            obj: &'a ObjectVariant,
        ) -> Result<&'a ObjectVariant, pdf_object::error::ObjectError> {
            match obj {
                ObjectVariant::Reference(obj_num) => self
                    .objects
                    .get(obj_num)
                    .ok_or(ObjectError::FailedResolveObjectReference { obj_num: *obj_num }),
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
        let resolver = MapResolver {
            objects: BTreeMap::from([(7, ObjectVariant::Stream(stream.clone()))]),
        };
        let mut cache = DefaultResourceCache::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let xobject = XObject::read_xobject(
            &ObjectVariant::Stream(stream.clone()),
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

    #[test]
    fn referenced_soft_mask_is_applied_to_image() {
        let image_stream = StreamObject::new(
            1,
            0,
            Box::new(Dictionary::new(BTreeMap::from([
                (
                    "Subtype".to_string(),
                    ObjectVariant::Name(b"Image".to_vec()),
                ),
                ("Width".to_string(), ObjectVariant::Integer(2)),
                ("Height".to_string(), ObjectVariant::Integer(1)),
                ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
                (
                    "ColorSpace".to_string(),
                    ObjectVariant::Name(b"DeviceGray".to_vec()),
                ),
                ("SMask".to_string(), ObjectVariant::Reference(2)),
            ]))),
            vec![0x20, 0xC0],
        );
        let mask_stream = StreamObject::new(
            2,
            0,
            Box::new(Dictionary::new(BTreeMap::from([
                (
                    "Subtype".to_string(),
                    ObjectVariant::Name(b"Image".to_vec()),
                ),
                ("Width".to_string(), ObjectVariant::Integer(2)),
                ("Height".to_string(), ObjectVariant::Integer(1)),
                ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
                (
                    "ColorSpace".to_string(),
                    ObjectVariant::Name(b"DeviceGray".to_vec()),
                ),
            ]))),
            vec![0x10, 0xE0],
        );
        let resolver = MapResolver {
            objects: BTreeMap::from([
                (1, ObjectVariant::Stream(image_stream.clone())),
                (2, ObjectVariant::Stream(mask_stream.clone())),
            ]),
        };
        let mut cache = DefaultResourceCache::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let xobject = XObject::read_xobject(
            &ObjectVariant::Stream(image_stream.clone()),
            &image_stream.dictionary,
            &image_stream,
            &resolver,
            &mut cache,
            &mut id_allocator,
        )
        .expect("a valid soft mask reference should decode");

        match xobject {
            XObject::Image(image) => {
                assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
                assert_eq!(
                    image.data,
                    vec![0x20, 0x20, 0x20, 0x10, 0xC0, 0xC0, 0xC0, 0xE0]
                );
            }
            XObject::Form(_) => panic!("expected an image xobject"),
        }
    }
}
