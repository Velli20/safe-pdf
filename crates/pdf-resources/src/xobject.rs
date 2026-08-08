use crate::{
    error::PdfPagesError, form::FormXObject, object_reader::ReadCycleTracker, resource::Resource,
    resource_cache::ResourceCache,
};
use pdf_content_stream::ContentStreamIdAllocator;
use pdf_image::{ImageXObject, PdfImageError};
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant, stream::StreamObject,
};
use std::rc::Rc;

/// Reads a stream-backed XObject with cycle protection.
pub(crate) fn read_xobject(
    content: &ObjectVariant,
    dictionary: &Dictionary,
    stream_data: &StreamObject,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<Option<Resource>, PdfPagesError> {
    if !cycle_tracker.begin_read(stream_data.object_number) {
        return Ok(None);
    }

    let result = read_xobject_inner(
        content,
        dictionary,
        stream_data,
        objects,
        cache,
        cycle_tracker,
        id_allocator,
    );
    cycle_tracker.end_read(stream_data.object_number);
    result.map(Some)
}

fn read_xobject_inner(
    content: &ObjectVariant,
    dictionary: &Dictionary,
    stream_data: &StreamObject,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<Resource, PdfPagesError> {
    match dictionary.required_str("Subtype", objects)? {
        "Image" => {
            if !has_valid_image_dimensions(dictionary, objects) {
                return Ok(Resource::UnavailableImage);
            }

            let soft_mask =
                resolve_image_soft_mask(dictionary, objects, cache, cycle_tracker, id_allocator)?;
            Ok(Resource::from(ImageXObject::read_xobject(
                dictionary,
                stream_data,
                objects,
                soft_mask,
            )?))
        }
        "Form" => FormXObject::read_xobject(
            content,
            dictionary,
            objects,
            cache,
            cycle_tracker,
            id_allocator,
        )
        .map(Resource::from),
        other => Err(PdfPagesError::UnsupportedXObjectSubtype {
            subtype: other.to_string(),
        }),
    }
}

/// Returns whether an image dictionary contains usable dimensions.
///
/// Malformed dimensions make the image impossible to decode, but should not
/// prevent otherwise valid page content from being loaded and rendered.
fn has_valid_image_dimensions(dictionary: &Dictionary, objects: &dyn ObjectResolver) -> bool {
    matches!(
        (
            dictionary.required_number::<usize>("Width", objects),
            dictionary.required_number::<usize>("Height", objects),
        ),
        (Ok(width), Ok(height)) if width > 0 && height > 0
    )
}

/// Resolves an image XObject `/SMask` entry through the page-level XObject reader.
///
/// `pdf-image` only decodes already-resolved image data, so page parsing owns the
/// PDF object lookup and XObject subtype validation. Routing mask streams through
/// [`read_xobject`] keeps soft-mask parsing on the same cycle-tracked
/// path as ordinary XObjects; recursive masks are therefore treated as absent.
fn resolve_image_soft_mask(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<Option<ImageXObject>, PdfImageError> {
    let Some(smask_obj) = dictionary.get("SMask") else {
        return Ok(None);
    };

    let resolved = objects.resolve_object(smask_obj)?;

    if let ObjectVariant::Name(name) = resolved
        && name.as_slice() == b"None"
    {
        return Ok(None);
    }

    let stream = resolved.try_stream(objects)?;

    match read_xobject(
        resolved,
        &stream.dictionary,
        stream,
        objects,
        cache,
        cycle_tracker,
        id_allocator,
    ) {
        Ok(Some(Resource::Image(image))) => Rc::try_unwrap(image)
            .map(Some)
            .map_err(|_| PdfImageError::InvalidSoftMaskXObject),
        Ok(Some(Resource::UnavailableImage)) => Ok(None),
        Ok(Some(_)) => Err(PdfImageError::InvalidSoftMaskXObject),
        Ok(None) => Ok(None),
        Err(PdfPagesError::Image(err)) => Err(err),
        Err(PdfPagesError::Object(err)) => Err(err.into()),
        Err(PdfPagesError::ColorSpace(err)) => Err(err.into()),
        Err(_) => Err(PdfImageError::InvalidSoftMaskXObject),
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

    use crate::{
        object_reader::ReadCycleTracker, resource::Resource, resource_cache::DefaultResourceCache,
    };

    use super::{has_valid_image_dimensions, read_xobject};

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
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let xobject = read_xobject(
            &ObjectVariant::Stream(stream.clone()),
            &stream.dictionary,
            &stream,
            &resolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        )
        .expect("self-referential soft masks should not fail image parsing")
        .expect("top-level image should be present");

        assert!(matches!(&xobject, Resource::Image(_)));
        if let Resource::Image(image) = xobject {
            assert_eq!(image.width, 1);
            assert_eq!(image.height, 1);
            assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::Gray8);
            assert_eq!(image.data.as_ref(), &[0xAA]);
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
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let xobject = read_xobject(
            &ObjectVariant::Stream(image_stream.clone()),
            &image_stream.dictionary,
            &image_stream,
            &resolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        )
        .expect("a valid soft mask reference should decode")
        .expect("top-level image should be present");

        match xobject {
            Resource::Image(image) => {
                assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
                assert_eq!(
                    image.data.as_ref(),
                    &[0x20, 0x20, 0x20, 0x10, 0xC0, 0xC0, 0xC0, 0xE0]
                );
            }
            Resource::UnavailableImage => panic!("expected a decoded image xobject"),
            _ => panic!("expected an image xobject"),
        }
    }

    #[test]
    fn malformed_image_dimensions_produce_unavailable_xobject() {
        let stream = StreamObject::new(
            9,
            0,
            Box::new(Dictionary::new(BTreeMap::from([
                (
                    "Subtype".to_string(),
                    ObjectVariant::Name(b"Image".to_vec()),
                ),
                ("Width".to_string(), ObjectVariant::Name(b"Height".to_vec())),
                ("BitsPerComponent".to_string(), ObjectVariant::Integer(8)),
                (
                    "ColorSpace".to_string(),
                    ObjectVariant::Name(b"DeviceGray".to_vec()),
                ),
            ]))),
            vec![0],
        );
        let resolver = MapResolver {
            objects: BTreeMap::new(),
        };
        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let xobject = read_xobject(
            &ObjectVariant::Stream(stream.clone()),
            &stream.dictionary,
            &stream,
            &resolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        )
        .expect("malformed image dimensions should be recoverable")
        .expect("the unavailable image resource should be preserved");

        assert!(matches!(xobject, Resource::UnavailableImage));
    }

    #[test]
    fn image_dimensions_must_be_positive_numbers() {
        let valid = Dictionary::new(BTreeMap::from([
            ("Width".to_string(), ObjectVariant::Integer(2)),
            ("Height".to_string(), ObjectVariant::Integer(3)),
        ]));
        let named_width = Dictionary::new(BTreeMap::from([
            ("Width".to_string(), ObjectVariant::Name(b"Height".to_vec())),
            ("Height".to_string(), ObjectVariant::Integer(3)),
        ]));
        let missing_height = Dictionary::new(BTreeMap::from([(
            "Width".to_string(),
            ObjectVariant::Integer(2),
        )]));
        let zero_width = Dictionary::new(BTreeMap::from([
            ("Width".to_string(), ObjectVariant::Integer(0)),
            ("Height".to_string(), ObjectVariant::Integer(3)),
        ]));
        let negative_height = Dictionary::new(BTreeMap::from([
            ("Width".to_string(), ObjectVariant::Integer(2)),
            ("Height".to_string(), ObjectVariant::Integer(-1)),
        ]));

        let resolver = pdf_object::object_resolver::PassthroughResolver;
        assert!(has_valid_image_dimensions(&valid, &resolver));
        assert!(!has_valid_image_dimensions(&named_width, &resolver));
        assert!(!has_valid_image_dimensions(&missing_height, &resolver));
        assert!(!has_valid_image_dimensions(&zero_width, &resolver));
        assert!(!has_valid_image_dimensions(&negative_height, &resolver));
    }
}
