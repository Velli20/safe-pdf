use crate::{error::PdfPagesError, form::FormXObject, resource::Resource, resources::Resources};
use pdf_font::PdfFontSpec;
use pdf_graphics::Image;
use pdf_image::{PdfImage, PdfImageError};
use pdf_object_reader::pdf_string::PdfString;
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ObjectHandle, ObjectReadError, ReadResult,
    object_variant::ObjectVariant, pdf_name::PdfName,
};
use std::sync::Arc;

/// The `/Subtype` of an XObject.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum XObjectSubtype {
    /// A self-contained content stream.
    Form,
    /// A sampled image.
    Image,
}

/// Reads the required `/Subtype` of an XObject stream or dictionary.
impl FromPdfObject for XObjectSubtype {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let subtype = context.dictionary()?.required::<PdfString>(b"Subtype")?;
        match subtype.as_ref() {
            b"Form" => Ok(Self::Form),
            b"Image" => Ok(Self::Image),
            other => Err(ObjectReadError::UnsupportedXObjectSubtype {
                subtype: String::from_utf8_lossy(other).into_owned(),
            }),
        }
    }
}

impl FromPdfObject for Resource {
    /// Streams must name a supported XObject subtype. A dictionary without one describes
    /// a font.
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let value = context.object();
        let subtype = match value {
            ObjectVariant::Stream(_) => Some(context.read::<XObjectSubtype>(value)?),
            _ => context.read::<XObjectSubtype>(value).ok(),
        };
        match subtype {
            None => context.read::<FontResource>(value).map(Self::from),
            Some(XObjectSubtype::Form) => context.read::<FormXObject>(value).map(Self::from),
            Some(XObjectSubtype::Image) => context.read::<ImageResource>(value).map(Self::from),
        }
    }
}

/// A font, with the resources a Type 3 font's glyph procedures paint with.
struct FontResource {
    font: PdfFontSpec,
    resources: Option<ObjectHandle<Resources>>,
}

impl FromPdfObject for FontResource {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let font: PdfFontSpec = context.read(context.object())?;
        let resources = match font.is_type3() {
            true => context.dictionary()?.optional_shared(b"Resources")?,
            false => None,
        };
        Ok(Self { font, resources })
    }
}

impl From<FontResource> for Resource {
    fn from(FontResource { font, resources }: FontResource) -> Self {
        Self::Font {
            font: Arc::new(font),
            resources,
        }
    }
}

/// An image XObject decoded into a raster, or unavailable when its dimensions are
/// malformed or its stream data cannot be decoded into samples.
enum ImageResource {
    Available(Image),
    Unavailable,
}

impl FromPdfObject for ImageResource {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let object = context.object();
        let image = match context.read::<PdfImage>(object) {
            Ok(image) => image,
            Err(error) if PdfImage::is_unreadable(&error) => return Ok(Self::Unavailable),
            Err(error) => return Err(error),
        };
        let SoftMaskImage(soft_mask) = context.read(object)?;
        match image.decode(soft_mask.as_deref()) {
            Ok(image) => Ok(Self::Available(image)),
            Err(error) if error.is_unreadable_data() => Ok(Self::Unavailable),
            Err(error) => Err(PdfPagesError::from(error).into()),
        }
    }
}

impl From<ImageResource> for Resource {
    fn from(image: ImageResource) -> Self {
        match image {
            ImageResource::Available(image) => Self::from(image),
            ImageResource::Unavailable => Self::UnavailableImage,
        }
    }
}

/// An image's `/SMask`.
///
/// `/None`, an unavailable mask, or a mask that refers back to its own image leaves the
/// image unmasked.
struct SoftMaskImage(Option<Arc<Image>>);

impl FromPdfObject for SoftMaskImage {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        if !context.contains(b"SMask")
            || context
                .lenient::<PdfName>(b"SMask")
                .is_some_and(|name| name.as_bytes() == b"None")
        {
            return Ok(Self(None));
        }
        match context.required::<Resource>(b"SMask") {
            Ok(Resource::Image(image)) => Ok(Self(Some(image))),
            Ok(Resource::UnavailableImage) => Ok(Self(None)),
            Err(error) if error.is_cyclic_reference() => Ok(Self(None)),
            Ok(_) => Err(PdfPagesError::from(PdfImageError::InvalidSoftMaskXObject).into()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use pdf_object_collection::object_collection::ObjectCollection;
    use pdf_object_reader::{
        ObjectReadError, dictionary::Dictionary, object_error::ObjectError, object_id::ObjectId,
        object_variant::ObjectVariant, stream::StreamObject,
    };
    use std::collections::BTreeMap;

    use crate::resource::Resource;

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

    fn image_dictionary(object_number: usize) -> Dictionary {
        Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"Subtype"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"Image",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
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
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"Image",
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                ),
                (Vec::from(b"Width"), ObjectVariant::Integer(2)),
                (Vec::from(b"Height"), ObjectVariant::Integer(1)),
                (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
                (
                    Vec::from(b"ColorSpace"),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"DeviceGray",
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
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
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"Image",
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                ),
                (Vec::from(b"Width"), ObjectVariant::Integer(2)),
                (Vec::from(b"Height"), ObjectVariant::Integer(1)),
                (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
                (
                    Vec::from(b"ColorSpace"),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"DeviceGray",
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
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
                pdf_object_reader::pdf_string::PdfString::from(
                    b"Unsupported",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            )])),
            vec![],
        );
        let resolver = MapResolver {
            objects: BTreeMap::from([(2, ObjectVariant::Stream(mask_stream))]),
        };

        let reader = pdf_object_reader::ObjectReader::new(&resolver);

        let result = reader.read::<Option<Resource>>(&ObjectVariant::Stream(image_stream.clone()));

        let Err(ObjectReadError::At { source, .. }) = result else {
            panic!("the error should name the /SMask entry");
        };
        assert!(matches!(
            *source,
            ObjectReadError::UnsupportedXObjectSubtype { subtype } if subtype == "Unsupported"
        ));
    }

    #[test]
    fn encoded_image_retries_filters_after_dependencies_are_resolved() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (
                Vec::from(b"DecodeParms"),
                ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(2, 0)),
            ),
            (
                Vec::from(b"Filter"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"ASCIIHexDecode",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (
                Vec::from(b"Subtype"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"Image",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
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
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"Image",
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                ),
                (
                    Vec::from(b"Width"),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"Height",
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                ),
                (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
                (
                    Vec::from(b"ColorSpace"),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"DeviceGray",
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
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
