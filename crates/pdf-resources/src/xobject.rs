use crate::{error::PdfPagesError, form::FormXObject, resource::Resource};
use pdf_image::{PdfImageError, read_xobject as decode_image_xobject};
use pdf_object_reader::object_lookup::ObjectLookupExt;
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
};

impl FromPdfObject for Resource {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let raw = context.object().object().clone();
        let dictionary = match raw.value() {
            pdf_object_reader::object_variant::ObjectVariant::Dictionary(dictionary) => dictionary,
            pdf_object_reader::object_variant::ObjectVariant::Stream(stream) => &stream.dictionary,
            other => {
                return Err(pdf_object_reader::object_error::ObjectError::TypeMismatch(
                    "Dictionary or Stream",
                    other.name(),
                )
                .into());
            }
        };
        let subtype = dictionary.optional_bytes(b"Subtype", context.source())?;
        if matches!(raw.value(), ObjectVariant::Dictionary(_))
            && !matches!(subtype, Some(b"Image" | b"Form"))
        {
            let font: pdf_font::PdfFontSpec = context.read(raw.value())?;
            let resources = if font.is_type3() {
                dictionary
                    .get(b"Resources")
                    .map(|value| context.read_shared(value))
                    .transpose()?
            } else {
                None
            };
            return Ok(Self::Font {
                font: std::sync::Arc::new(font),
                resources,
            });
        }
        match dictionary.required_bytes(b"Subtype", context.source())? {
            b"Form" => Ok(Self::from(context.read::<FormXObject>(raw.value())?)),
            b"Image" => {
                if !dictionary
                    .required_size(context.source())
                    .is_ok_and(|size| size.is_valid())
                {
                    return Ok(Self::UnavailableImage);
                }
                let ObjectVariant::Stream(stream) = raw.value() else {
                    return Err(pdf_object_reader::object_error::ObjectError::TypeMismatch(
                        "Stream",
                        raw.value().name(),
                    )
                    .into());
                };
                let soft_mask = match dictionary.get(b"SMask") {
                    None => None,
                    Some(value) => {
                        let resolved = context.source().resolve_object(value)?;
                        if matches!(resolved, ObjectVariant::Name(name) if name == b"None") {
                            None
                        } else {
                            match context.read::<Resource>(value) {
                                Ok(Resource::Image(image)) => Some(image),
                                Ok(Resource::UnavailableImage) => None,
                                Err(pdf_object_reader::ObjectReadError::CyclicReference {
                                    ..
                                }) => None,
                                Err(error) => return Err(error),
                                _ => {
                                    return Err(PdfPagesError::from(
                                        PdfImageError::InvalidSoftMaskXObject,
                                    )
                                    .into());
                                }
                            }
                        }
                    }
                };
                Ok(Self::from(
                    decode_image_xobject(
                        dictionary,
                        stream,
                        context.source(),
                        soft_mask.as_deref(),
                    )
                    .map_err(PdfPagesError::from)?,
                ))
            }
            subtype => Err(PdfPagesError::UnsupportedXObjectSubtype {
                subtype: String::from_utf8_lossy(subtype).into_owned(),
            }
            .into()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use pdf_object_collection::object_collection::ObjectCollection;
    use pdf_object_reader::{
        dictionary::Dictionary, object_error::ObjectError, object_id::ObjectId,
        object_resolver::ObjectResolver, object_variant::ObjectVariant, stream::StreamObject,
    };
    use std::collections::BTreeMap;

    use crate::{error::PdfPagesError, resource::Resource};

    fn object_id(number: usize) -> ObjectId {
        ObjectId {
            number,
            generation: 0,
        }
    }

    struct MapResolver {
        objects: BTreeMap<usize, ObjectVariant>,
    }

    impl pdf_object_reader::ObjectSource for MapResolver {
        type Error = ObjectError;
        fn read_object(
            &self,
            id: pdf_object_reader::object_id::ObjectId,
        ) -> Result<Option<pdf_object_reader::pdf_object::PdfObject>, Self::Error> {
            Ok(self
                .objects
                .get(&id.number())
                .cloned()
                .map(pdf_object_reader::pdf_object::PdfObject::new))
        }
    }

    impl ObjectResolver for MapResolver {
        fn resolve_object<'a>(
            &'a self,
            obj: &'a ObjectVariant,
        ) -> Result<&'a ObjectVariant, pdf_object_reader::object_error::ObjectError> {
            match obj {
                ObjectVariant::Reference(obj_num) => self.objects.get(&obj_num.number).ok_or(
                    ObjectError::FailedResolveObjectReference {
                        obj_num: obj_num.number,
                    },
                ),
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
            (
                Vec::from(b"SMask"),
                ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(
                    object_number,
                    0,
                )),
            ),
        ]))
    }

    #[test]
    fn self_referential_soft_mask_is_treated_as_absent() {
        let stream = StreamObject::new(7, 0, image_dictionary(7), vec![0xAA]);
        let resolver = MapResolver {
            objects: BTreeMap::from([(7, ObjectVariant::Stream(stream.clone()))]),
        };

        let reader = pdf_object_reader::ObjectReader::new(&resolver);

        let xobject = reader
            .read_indirect::<Option<Resource>>(pdf_object_reader::object_id::ObjectId::new(7, 0))
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
            Dictionary::new(BTreeMap::from([
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
                (
                    Vec::from(b"SMask"),
                    ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(2, 0)),
                ),
            ])),
            vec![0x20, 0xC0],
        );
        let mask_stream = StreamObject::new(
            2,
            0,
            Dictionary::new(BTreeMap::from([
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
            ])),
            vec![0x10, 0xE0],
        );
        let resolver = MapResolver {
            objects: BTreeMap::from([
                (1, ObjectVariant::Stream(image_stream.clone())),
                (2, ObjectVariant::Stream(mask_stream.clone())),
            ]),
        };

        let reader = pdf_object_reader::ObjectReader::new(&resolver);

        let xobject = reader
            .read::<Option<Resource>>(&ObjectVariant::Stream(image_stream.clone()))
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
        let image_stream = StreamObject::new(1, 0, image_dictionary(2), vec![0xAA]);
        let mask_stream = StreamObject::new(
            2,
            0,
            Dictionary::new(BTreeMap::from([(
                Vec::from(b"Subtype"),
                ObjectVariant::Name(b"Unsupported".to_vec()),
            )])),
            vec![],
        );
        let resolver = MapResolver {
            objects: BTreeMap::from([(2, ObjectVariant::Stream(mask_stream))]),
        };

        let reader = pdf_object_reader::ObjectReader::new(&resolver);

        let result = reader.read::<Option<Resource>>(&ObjectVariant::Stream(image_stream.clone()));

        assert!(matches!(
            result,
            Err(pdf_object_reader::ObjectReadError::Decode { source, .. })
                if matches!(source.downcast_ref::<PdfPagesError>(), Some(PdfPagesError::UnsupportedXObjectSubtype { subtype }) if subtype == "Unsupported")
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
            (
                Vec::from(b"DecodeParms"),
                ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(2, 0)),
            ),
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
        let stream = StreamObject::new_encoded(1, 0, dictionary, b"2A>".to_vec());
        let mut objects = ObjectCollection::default();

        objects
            .insert(object_id(1), ObjectVariant::Stream(stream))
            .expect("unresolved filter parameters should preserve the image stream");
        objects
            .insert(
                object_id(2),
                ObjectVariant::Dictionary(Dictionary::new(
                    BTreeMap::<Vec<u8>, ObjectVariant>::new(),
                )),
            )
            .expect("decode parameters should be inserted");

        let content = objects.get(1).expect("image stream should be retained");
        let ObjectVariant::Stream(stream) = content else {
            panic!("expected an image stream");
        };
        assert!(!stream.filters_applied());

        let reader = pdf_object_reader::ObjectReader::new(&objects);
        let xobject = reader
            .read::<Option<Resource>>(content)
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
            Dictionary::new(BTreeMap::from([
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
            ])),
            vec![0],
        );
        let resolver = MapResolver {
            objects: BTreeMap::new(),
        };

        let reader = pdf_object_reader::ObjectReader::new(&resolver);

        let xobject = reader
            .read::<Option<Resource>>(&ObjectVariant::Stream(stream.clone()))
            .expect("malformed image dimensions should be recoverable")
            .expect("the unavailable image resource should be preserved");

        assert!(matches!(xobject, Resource::UnavailableImage));
    }
}
