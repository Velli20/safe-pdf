use crate::{
    error::PdfPagesError, form::FormXObject, object_reader::ReadCycleTracker, resource::Resource,
    resource_cache::ResourceCache,
};
use pdf_content_stream::ContentStreamIdAllocator;
use pdf_graphics::Image;
use pdf_image::{PdfImageError, read_xobject as decode_image_xobject};
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
    match dictionary.required_bytes(b"Subtype", objects)? {
        b"Image" => {
            // Malformed dimensions make the image impossible to decode, but should not
            // prevent otherwise valid page content from being loaded and rendered.
            if !dictionary
                .required_size(objects)
                .is_ok_and(|size| size.is_valid())
            {
                return Ok(Resource::UnavailableImage);
            }

            let soft_mask =
                resolve_image_soft_mask(dictionary, objects, cache, cycle_tracker, id_allocator)?;
            Ok(Resource::from(decode_image_xobject(
                dictionary,
                stream_data,
                objects,
                soft_mask.as_ref(),
            )?))
        }
        b"Form" => FormXObject::read_xobject(
            content,
            dictionary,
            objects,
            cache,
            cycle_tracker,
            id_allocator,
        )
        .map(Resource::from),
        other => Err(PdfPagesError::UnsupportedXObjectSubtype {
            subtype: String::from_utf8_lossy(other).into_owned(),
        }),
    }
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
) -> Result<Option<Image>, PdfPagesError> {
    let Some(smask_obj) = dictionary.get(b"SMask") else {
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
    )? {
        Some(Resource::Image(image)) => Rc::try_unwrap(image)
            .map(Some)
            .map_err(|_| PdfImageError::InvalidSoftMaskXObject.into()),
        Some(Resource::UnavailableImage) | None => Ok(None),
        Some(_) => Err(PdfImageError::InvalidSoftMaskXObject.into()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use pdf_content_stream::ContentStreamIdAllocator;
    use pdf_object::{
        dictionary::Dictionary, error::ObjectError, object_id::PdfObjectId,
        object_resolver::ObjectResolver, object_variant::ObjectVariant, stream::StreamObject,
    };
    use pdf_object_collection::object_collection::ObjectCollection;
    use std::collections::BTreeMap;

    use crate::{
        error::PdfPagesError, object_reader::ReadCycleTracker, resource::Resource,
        resource_cache::DefaultResourceCache,
    };

    use super::read_xobject;

    fn object_id(number: usize) -> PdfObjectId {
        PdfObjectId {
            number,
            generation: 0,
        }
    }

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
                Vec::from(b"Subtype"),
                ObjectVariant::Name(b"Image".to_vec()),
            ),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (Vec::from(b"SMask"), ObjectVariant::Reference(object_number)),
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
                    Vec::from(b"Subtype"),
                    ObjectVariant::Name(b"Image".to_vec()),
                ),
                (Vec::from(b"Width"), ObjectVariant::Integer(2)),
                (Vec::from(b"Height"), ObjectVariant::Integer(1)),
                (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
                (
                    Vec::from(b"ColorSpace"),
                    ObjectVariant::Name(b"DeviceGray".to_vec()),
                ),
                (Vec::from(b"SMask"), ObjectVariant::Reference(2)),
            ]))),
            vec![0x20, 0xC0],
        );
        let mask_stream = StreamObject::new(
            2,
            0,
            Box::new(Dictionary::new(BTreeMap::from([
                (
                    Vec::from(b"Subtype"),
                    ObjectVariant::Name(b"Image".to_vec()),
                ),
                (Vec::from(b"Width"), ObjectVariant::Integer(2)),
                (Vec::from(b"Height"), ObjectVariant::Integer(1)),
                (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
                (
                    Vec::from(b"ColorSpace"),
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
    fn soft_mask_xobject_error_is_propagated() {
        let image_stream = StreamObject::new(1, 0, Box::new(image_dictionary(2)), vec![0xAA]);
        let mask_stream = StreamObject::new(
            2,
            0,
            Box::new(Dictionary::new(BTreeMap::from([(
                Vec::from(b"Subtype"),
                ObjectVariant::Name(b"Unsupported".to_vec()),
            )]))),
            vec![],
        );
        let resolver = MapResolver {
            objects: BTreeMap::from([(2, ObjectVariant::Stream(mask_stream))]),
        };
        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let result = read_xobject(
            &ObjectVariant::Stream(image_stream.clone()),
            &image_stream.dictionary,
            &image_stream,
            &resolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        );

        assert!(matches!(
            result,
            Err(PdfPagesError::UnsupportedXObjectSubtype { subtype })
                if subtype == "Unsupported"
        ));
    }

    #[test]
    fn encoded_image_retries_filters_after_dependencies_are_resolved() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (Vec::from(b"DecodeParms"), ObjectVariant::Reference(2)),
            (
                Vec::from(b"Filter"),
                ObjectVariant::Name(b"ASCIIHexDecode".to_vec()),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (
                Vec::from(b"Subtype"),
                ObjectVariant::Name(b"Image".to_vec()),
            ),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));
        let stream = StreamObject::new_encoded(1, 0, Box::new(dictionary), b"2A>".to_vec());
        let mut objects = ObjectCollection::default();

        objects
            .insert(object_id(1), ObjectVariant::Stream(stream))
            .expect("unresolved filter parameters should preserve the image stream");
        objects
            .insert(
                object_id(2),
                ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::<
                    Vec<u8>,
                    ObjectVariant,
                >::new()))),
            )
            .expect("decode parameters should be inserted");

        let content = objects.get(1).expect("image stream should be retained");
        let ObjectVariant::Stream(stream) = content else {
            panic!("expected an image stream");
        };
        assert!(!stream.filters_applied());

        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();
        let xobject = read_xobject(
            content,
            &stream.dictionary,
            stream,
            &objects,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        )
        .expect("image decoding should retry the filter chain")
        .expect("image should remain available");

        let Resource::Image(image) = xobject else {
            panic!("expected a decoded image xobject");
        };
        assert_eq!(image.data.as_ref(), &[0x2A]);
    }

    #[test]
    fn malformed_image_dimensions_produce_unavailable_xobject() {
        let stream = StreamObject::new(
            9,
            0,
            Box::new(Dictionary::new(BTreeMap::from([
                (
                    Vec::from(b"Subtype"),
                    ObjectVariant::Name(b"Image".to_vec()),
                ),
                (Vec::from(b"Width"), ObjectVariant::Name(b"Height".to_vec())),
                (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
                (
                    Vec::from(b"ColorSpace"),
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
}
