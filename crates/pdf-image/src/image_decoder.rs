use bytes::Bytes;
use pdf_filter::filter::decode_with_resolver;
use pdf_graphics::Image;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver, stream::StreamObject};

use crate::InlineImage;
use crate::decoded_samples::DecodedSamples;
use crate::error::PdfImageError;
use crate::image_metadata::ImageMetadata;

/// Parses an Image XObject from a PDF stream dictionary and data.
pub fn read_xobject(
    dictionary: &Dictionary,
    stream_data: &StreamObject,
    objects: &dyn ObjectResolver,
    soft_mask: Option<&Image>,
) -> Result<Image, PdfImageError> {
    let metadata = ImageMetadata::from_dictionary(dictionary, objects)?;
    let decoded = if stream_data.filters_applied() {
        stream_data.shared_data()
    } else {
        decode_with_resolver(stream_data, objects)?
    };

    decode_normalized_image_with_metadata(decoded, soft_mask, &metadata)
}

/// Decodes an inline image, including its filter chain and normalized sample data.
pub fn decode_inline_image(
    image: &InlineImage,
    soft_mask: Option<&Image>,
) -> Result<Image, PdfImageError> {
    decode_normalized_image_with_metadata(image.shared_data(), soft_mask, image.metadata())
}

/// Decodes a normalized image dictionary and shared raw bytes into a raster image.
///
/// The dictionary must already use canonical image keys such as `Width`,
/// `Height`, `BitsPerComponent`, and `ColorSpace`.
pub fn decode_normalized_image(
    dictionary: &Dictionary,
    raw_data: Bytes,
    objects: &dyn ObjectResolver,
    soft_mask: Option<&Image>,
) -> Result<Image, PdfImageError> {
    let metadata = ImageMetadata::from_dictionary(dictionary, objects)?;
    decode_normalized_image_with_metadata(raw_data, soft_mask, &metadata)
}

fn decode_normalized_image_with_metadata(
    raw_data: Bytes,
    soft_mask: Option<&Image>,
    metadata: &ImageMetadata,
) -> Result<Image, PdfImageError> {
    let decoded_samples = DecodedSamples::decode(raw_data, metadata)?;
    let DecodedSamples {
        num_color_components,
        image_data,
        is_rgba,
    } = decoded_samples;
    if is_rgba {
        return Ok(image_from_rgba(
            image_data,
            metadata.size.width(),
            metadata.size.height(),
            soft_mask,
        ));
    }
    Ok(Image::from_decoded_samples(
        image_data,
        metadata.size.width(),
        metadata.size.height(),
        num_color_components,
        soft_mask,
    ))
}

/// Builds a render image from color-space-converted RGBA and combines its alpha with `/SMask`.
fn image_from_rgba(data: Bytes, width: usize, height: usize, soft_mask: Option<&Image>) -> Image {
    let byte_len = width
        .saturating_mul(height)
        .saturating_mul(4)
        .min(data.len());
    if soft_mask.is_none() && byte_len == data.len() {
        return Image {
            data,
            width,
            height,
            pixel_format: pdf_graphics::PixelFormat::RGBA8888,
        };
    }

    let mut rgba = data.get(..byte_len).map_or_else(Vec::new, <[u8]>::to_vec);
    if let Some(mask) = soft_mask {
        for (pixel, mask_alpha) in rgba.chunks_exact_mut(4).zip(soft_mask_alphas(mask)) {
            if let [_, _, _, alpha] = pixel {
                let combined = u16::from(*alpha).saturating_mul(u16::from(mask_alpha)) / 255;
                *alpha = u8::try_from(combined).unwrap_or(u8::MAX);
            }
        }
    }
    Image {
        data: rgba.into(),
        width,
        height,
        pixel_format: pdf_graphics::PixelFormat::RGBA8888,
    }
}

fn soft_mask_alphas(mask: &Image) -> Box<dyn Iterator<Item = u8> + '_> {
    match mask.pixel_format {
        pdf_graphics::PixelFormat::Gray8 => Box::new(mask.data.iter().copied()),
        pdf_graphics::PixelFormat::RGBA8888 => {
            Box::new(mask.data.chunks_exact(4).filter_map(|pixel| match pixel {
                [gray, _, _, alpha] => {
                    let combined = u16::from(*gray).saturating_mul(u16::from(*alpha)) / 255;
                    Some(u8::try_from(combined).unwrap_or(u8::MAX))
                }
                _ => None,
            }))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use bytes::Bytes;
    use pdf_object::{
        dictionary::Dictionary, error::ObjectError, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    use pdf_graphics::{Image, PixelFormat};

    use super::{InlineImage, decode_inline_image, decode_normalized_image, read_xobject};
    use crate::error::PdfImageError;

    fn name(value: &str) -> ObjectVariant {
        ObjectVariant::Name(value.as_bytes().to_vec())
    }

    fn fixture_cal_rgb() -> ObjectVariant {
        ObjectVariant::Array(vec![
            name("CalRGB"),
            ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([
                (
                    Vec::from(b"WhitePoint"),
                    ObjectVariant::Array(vec![
                        ObjectVariant::Real(0.9505),
                        ObjectVariant::Real(1.0),
                        ObjectVariant::Real(1.089),
                    ]),
                ),
                (
                    Vec::from(b"Matrix"),
                    ObjectVariant::Array(vec![
                        ObjectVariant::Real(0.9505),
                        ObjectVariant::Real(1.0),
                        ObjectVariant::Real(1.089),
                        ObjectVariant::Integer(0),
                        ObjectVariant::Integer(0),
                        ObjectVariant::Integer(0),
                        ObjectVariant::Integer(0),
                        ObjectVariant::Integer(0),
                        ObjectVariant::Integer(0),
                    ]),
                ),
            ]))),
        ])
    }

    fn fixture_device_n() -> ObjectVariant {
        let function = StreamObject::new(
            1,
            0,
            Dictionary::new(BTreeMap::from([
                (Vec::from(b"FunctionType"), ObjectVariant::Integer(4)),
                (
                    Vec::from(b"Domain"),
                    ObjectVariant::Array(
                        (0..4)
                            .flat_map(|_| [ObjectVariant::Integer(0), ObjectVariant::Integer(1)])
                            .collect(),
                    ),
                ),
                (
                    Vec::from(b"Range"),
                    ObjectVariant::Array(
                        (0..3)
                            .flat_map(|_| [ObjectVariant::Integer(0), ObjectVariant::Integer(1)])
                            .collect(),
                    ),
                ),
            ])),
            b"4 3 roll pop".to_vec(),
        );

        ObjectVariant::Array(vec![
            name("DeviceN"),
            ObjectVariant::Array(vec![name("IBM"), name("None"), name("None"), name("None")]),
            fixture_cal_rgb(),
            ObjectVariant::Stream(function),
        ])
    }

    #[test]
    fn read_xobject_uses_predecoded_filtered_stream_data() {
        let dictionary = filtered_gray_dictionary();
        let stream = StreamObject::new(1, 0, dictionary.clone(), vec![0x2A]);

        let image = read_xobject(&dictionary, &stream, &PassthroughResolver, None)
            .expect("predecoded image data should not be filtered again");

        assert_eq!(image.data.as_ptr(), stream.data.as_ptr());
        assert_eq!(image.data.as_ref(), &[0x2A]);
    }

    #[test]
    fn read_xobject_decodes_encoded_stream_data() {
        let dictionary = filtered_gray_dictionary();
        let stream = StreamObject::new_encoded(1, 0, dictionary.clone(), b"2A>".to_vec());

        let image = read_xobject(&dictionary, &stream, &PassthroughResolver, None)
            .expect("encoded image data should have its filter applied");

        assert_eq!(image.data.as_ref(), &[0x2A]);
    }

    #[test]
    fn decode_normalized_1bpc_gray_inverts_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(1)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (
                Vec::from(b"Decode"),
                ObjectVariant::Array(vec![ObjectVariant::Integer(1), ObjectVariant::Integer(0)]),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(4)),
        ]));

        let image = decode_normalized_image(
            &dictionary,
            vec![0b1010_0000].into(),
            &PassthroughResolver,
            None,
        )
        .expect("1-bpc decoded grayscale image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(image.data.as_ref(), &[0x00, 0xFF, 0x00, 0xFF]);
    }

    #[test]
    fn decode_normalized_8bpc_gray_remaps_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (
                Vec::from(b"Decode"),
                ObjectVariant::Array(vec![ObjectVariant::Integer(0), ObjectVariant::Real(0.5)]),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));

        let image =
            decode_normalized_image(&dictionary, vec![0, 255].into(), &PassthroughResolver, None)
                .expect("8-bpc decoded grayscale image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(image.data.as_ref(), &[0x00, 0x80]);
    }

    #[test]
    fn decode_normalized_indexed_image_applies_decode_before_lookup() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(1)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Array(vec![
                    ObjectVariant::Name(b"Indexed".to_vec()),
                    ObjectVariant::Name(b"DeviceRGB".to_vec()),
                    ObjectVariant::Integer(1),
                    ObjectVariant::HexString(vec![10, 11, 12, 20, 21, 22]),
                ]),
            ),
            (
                Vec::from(b"Decode"),
                ObjectVariant::Array(vec![ObjectVariant::Integer(1), ObjectVariant::Integer(0)]),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));

        let image = decode_normalized_image(
            &dictionary,
            vec![0b1000_0000].into(),
            &PassthroughResolver,
            None,
        )
        .expect("decoded indexed image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(image.data.as_ref(), &[10, 11, 12, 255, 20, 21, 22, 255]);
    }

    #[test]
    fn indexed_device_n_palette_is_converted_through_cal_rgb() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Array(vec![
                    name("Indexed"),
                    fixture_device_n(),
                    ObjectVariant::Integer(0),
                    ObjectVariant::HexString(vec![0, 255, 255, 255]),
                ]),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));

        let image =
            decode_normalized_image(&dictionary, vec![0].into(), &PassthroughResolver, None)
                .expect("Indexed DeviceN image should convert through its alternate space");

        assert_eq!(image.pixel_format, PixelFormat::RGBA8888);
        assert_eq!(image.data.as_ref(), &[255, 255, 255, 255]);
    }

    #[test]
    fn direct_four_component_device_n_is_not_interpreted_as_cmyk() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (Vec::from(b"ColorSpace"), fixture_device_n()),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));

        let image = decode_normalized_image(
            &dictionary,
            vec![0, 255, 255, 255].into(),
            &PassthroughResolver,
            None,
        )
        .expect("DeviceN image should convert through its tint transform");

        assert_eq!(image.data.as_ref(), &[255, 255, 255, 255]);
    }

    #[test]
    fn indexed_device_cmyk_keeps_device_fast_path() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Array(vec![
                    name("Indexed"),
                    name("DeviceCMYK"),
                    ObjectVariant::Integer(0),
                    ObjectVariant::HexString(vec![0, 0, 0, 0]),
                ]),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));

        let image =
            decode_normalized_image(&dictionary, vec![0].into(), &PassthroughResolver, None)
                .expect("Indexed DeviceCMYK image should retain its existing conversion");

        assert_eq!(image.data.as_ref(), &[255, 255, 255, 255]);
    }

    #[test]
    fn color_space_alpha_is_combined_with_soft_mask() {
        let tint_function = StreamObject::new(
            2,
            0,
            Dictionary::new(BTreeMap::from([
                (Vec::from(b"FunctionType"), ObjectVariant::Integer(4)),
                (
                    Vec::from(b"Domain"),
                    ObjectVariant::Array(vec![
                        ObjectVariant::Integer(0),
                        ObjectVariant::Integer(1),
                    ]),
                ),
                (
                    Vec::from(b"Range"),
                    ObjectVariant::Array(
                        (0..3)
                            .flat_map(|_| [ObjectVariant::Integer(0), ObjectVariant::Integer(1)])
                            .collect(),
                    ),
                ),
            ])),
            b"pop 1 1 1".to_vec(),
        );
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Array(vec![
                    name("Separation"),
                    name("None"),
                    name("DeviceRGB"),
                    ObjectVariant::Stream(tint_function),
                ]),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));
        let soft_mask = Image {
            data: vec![128].into(),
            width: 1,
            height: 1,
            pixel_format: PixelFormat::Gray8,
        };

        let image = decode_normalized_image(
            &dictionary,
            vec![255].into(),
            &PassthroughResolver,
            Some(&soft_mask),
        )
        .expect("Separation alpha should survive soft-mask application");

        assert_eq!(image.data.as_ref(), &[0, 0, 0, 0]);
    }

    #[test]
    fn decode_normalized_image_rejects_invalid_decode_length() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (
                Vec::from(b"Decode"),
                ObjectVariant::Array(vec![ObjectVariant::Integer(0)]),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));

        let err = decode_normalized_image(&dictionary, vec![0].into(), &PassthroughResolver, None)
            .expect_err("invalid /Decode length should fail");

        assert!(matches!(
            err,
            PdfImageError::InvalidDecodeLength {
                expected_values: 2,
                actual_values: 1
            }
        ));
    }

    #[test]
    fn decode_normalized_image_without_decode_preserves_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));

        let samples = Bytes::from_static(&[12, 34]);
        let image =
            decode_normalized_image(&dictionary, samples.clone(), &PassthroughResolver, None)
                .expect("grayscale image without /Decode should decode");

        assert_eq!(image.data.as_ptr(), samples.as_ptr());
        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(image.data.as_ref(), &[12, 34]);
    }

    #[test]
    fn decode_normalized_image_trims_trailing_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));
        let samples = Bytes::from_static(&[12, 34, 56]);

        let image =
            decode_normalized_image(&dictionary, samples.clone(), &PassthroughResolver, None)
                .expect("trailing samples should be ignored");

        assert_eq!(image.data.as_ptr(), samples.as_ptr());
        assert_eq!(image.data.as_ref(), &[12, 34]);
    }

    #[test]
    fn decode_normalized_image_mask_defaults_bits_per_component_to_one() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"ImageMask"), ObjectVariant::Boolean(true)),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(4)),
        ]));

        let image = decode_normalized_image(
            &dictionary,
            vec![0b1010_0000].into(),
            &PassthroughResolver,
            None,
        )
        .expect("image masks should default missing BitsPerComponent to 1");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(image.data.as_ref(), &[0x00, 0xFF, 0x00, 0xFF]);
    }

    #[test]
    fn decode_normalized_jpx_image_without_bits_per_component_infers_rgb_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"Filter"),
                ObjectVariant::Name(b"JPXDecode".to_vec()),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));

        let image = decode_normalized_image(
            &dictionary,
            vec![1, 2, 3, 4, 5, 6].into(),
            &PassthroughResolver,
            None,
        )
        .expect("JPX images should decode without BitsPerComponent when already expanded");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(image.data.as_ref(), &[1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn decode_normalized_non_mask_image_still_requires_bits_per_component() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));

        let err = decode_normalized_image(&dictionary, vec![0].into(), &PassthroughResolver, None)
            .expect_err("non-mask images should still require BitsPerComponent");

        assert!(matches!(
            err,
            PdfImageError::Object(ObjectError::MissingRequiredKey { ref key }) if key == "BitsPerComponent"
        ));
    }

    #[test]
    fn decode_normalized_dct_cmyk_accepts_preconverted_rgb_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceCMYK".to_vec()),
            ),
            (
                Vec::from(b"Filter"),
                ObjectVariant::Name(b"DCTDecode".to_vec()),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));

        let image = decode_normalized_image(
            &dictionary,
            vec![10, 20, 30, 40, 50, 60].into(),
            &PassthroughResolver,
            None,
        )
        .expect("DCT-decoded RGB bytes should not be validated as CMYK samples");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(image.data.as_ref(), &[10, 20, 30, 255, 40, 50, 60, 255]);
    }

    #[test]
    fn decode_normalized_non_dct_cmyk_still_rejects_rgb_sized_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceCMYK".to_vec()),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));

        let err = decode_normalized_image(
            &dictionary,
            vec![10, 20, 30, 40, 50, 60].into(),
            &PassthroughResolver,
            None,
        )
        .expect_err("non-DCT CMYK image data should still require four components");

        assert!(matches!(
            err,
            PdfImageError::TruncatedImageData {
                expected_bytes: 8,
                actual_bytes: 6
            }
        ));
    }

    #[test]
    fn decode_normalized_dct_single_pixel_expands_to_declared_size() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceRGB".to_vec()),
            ),
            (
                Vec::from(b"Filter"),
                ObjectVariant::Name(b"DCTDecode".to_vec()),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));

        let image = decode_normalized_image(
            &dictionary,
            vec![0xAA, 0x10, 0x20].into(),
            &PassthroughResolver,
            None,
        )
        .expect("single decoded DCT pixel should expand to the declared image size");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            image.data.as_ref(),
            &[0xAA, 0x10, 0x20, 255, 0xAA, 0x10, 0x20, 255]
        );
    }

    #[test]
    fn decode_normalized_image_applies_resolved_soft_mask() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));
        let soft_mask = Image {
            width: 2,
            height: 1,
            data: vec![0x10, 0xE0].into(),
            pixel_format: PixelFormat::Gray8,
        };

        let image = decode_normalized_image(
            &dictionary,
            vec![0x20, 0xC0].into(),
            &PassthroughResolver,
            Some(&soft_mask),
        )
        .expect("resolved soft mask should be applied");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            image.data.as_ref(),
            &[0x20, 0x20, 0x20, 0x10, 0xC0, 0xC0, 0xC0, 0xE0]
        );
    }

    #[test]
    fn decode_inline_image_applies_filter_chain_before_samples() {
        let image = InlineImage::new(
            Dictionary::new(BTreeMap::from([
                (Vec::from(b"BPC"), ObjectVariant::Integer(8)),
                (
                    Vec::from(b"CS"),
                    ObjectVariant::Name(b"DeviceGray".to_vec()),
                ),
                (
                    Vec::from(b"F"),
                    ObjectVariant::Name(b"ASCIIHexDecode".to_vec()),
                ),
                (Vec::from(b"H"), ObjectVariant::Integer(1)),
                (Vec::from(b"W"), ObjectVariant::Integer(1)),
            ])),
            b"2A>".to_vec(),
            &PassthroughResolver,
        )
        .expect("inline image filters should decode");

        let decoded = decode_inline_image(&image, None).expect("inline image should decode");

        assert_eq!(decoded.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(decoded.data.as_ref(), &[0x2A]);
    }

    #[test]
    fn decode_inline_image_accepts_abbreviated_gray_color_space() {
        let image = InlineImage::new(
            Dictionary::new(BTreeMap::from([
                (Vec::from(b"BPC"), ObjectVariant::Integer(1)),
                (Vec::from(b"CS"), ObjectVariant::Name(b"G".to_vec())),
                (Vec::from(b"H"), ObjectVariant::Integer(1)),
                (Vec::from(b"W"), ObjectVariant::Integer(4)),
            ])),
            vec![0b1010_0000],
            &PassthroughResolver,
        )
        .expect("inline image should be constructed");

        let decoded = decode_inline_image(&image, None)
            .expect("inline image with abbreviated gray color space should decode");

        assert_eq!(decoded.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(decoded.data.as_ref(), &[0xFF, 0x00, 0xFF, 0x00]);
    }

    #[test]
    fn decode_inline_image_preserves_unfiltered_shared_samples() {
        let image = InlineImage::new(
            Dictionary::new(BTreeMap::from([
                (Vec::from(b"BPC"), ObjectVariant::Integer(8)),
                (Vec::from(b"CS"), ObjectVariant::Name(b"G".to_vec())),
                (Vec::from(b"H"), ObjectVariant::Integer(1)),
                (Vec::from(b"W"), ObjectVariant::Integer(2)),
            ])),
            vec![12, 34],
            &PassthroughResolver,
        )
        .expect("inline image should be constructed");
        let samples = image.shared_data();

        let decoded =
            decode_inline_image(&image, None).expect("unfiltered inline image should decode");

        assert_eq!(decoded.data.as_ptr(), samples.as_ptr());
        assert_eq!(decoded.data.as_ref(), &[12, 34]);
    }

    #[test]
    fn decode_inline_image_accepts_abbreviated_indexed_color_space() {
        let image = InlineImage::new(
            Dictionary::new(BTreeMap::from([
                (Vec::from(b"BPC"), ObjectVariant::Integer(1)),
                (
                    Vec::from(b"CS"),
                    ObjectVariant::Array(vec![
                        ObjectVariant::Name(b"I".to_vec()),
                        ObjectVariant::Name(b"RGB".to_vec()),
                        ObjectVariant::Integer(1),
                        ObjectVariant::HexString(vec![10, 11, 12, 20, 21, 22]),
                    ]),
                ),
                (Vec::from(b"H"), ObjectVariant::Integer(1)),
                (Vec::from(b"W"), ObjectVariant::Integer(4)),
            ])),
            vec![0b1010_0000],
            &PassthroughResolver,
        )
        .expect("inline image should be constructed");

        let decoded = decode_inline_image(&image, None)
            .expect("inline image with abbreviated indexed color space should decode");

        assert_eq!(decoded.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            decoded.data.as_ref(),
            &[
                20, 21, 22, 255, 10, 11, 12, 255, 20, 21, 22, 255, 10, 11, 12, 255
            ]
        );
    }

    #[test]
    fn decode_normalized_1bpc_image_expands_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(1)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(8)),
        ]));

        let image = decode_normalized_image(
            &dictionary,
            vec![0b1011_0010].into(),
            &PassthroughResolver,
            None,
        )
        .expect("1-bpc image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(
            image.data.as_ref(),
            &[0xFF, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0x00]
        );
    }

    #[test]
    fn decode_normalized_indexed_image_1bpc_expands_samples() {
        let dictionary = indexed_dictionary(1);
        let image = decode_normalized_image(
            &dictionary,
            vec![0b1010_0000].into(),
            &PassthroughResolver,
            None,
        )
        .expect("1-bpc indexed image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            image.data.as_ref(),
            &[
                20, 21, 22, 255, 10, 11, 12, 255, 20, 21, 22, 255, 10, 11, 12, 255
            ]
        );
    }

    #[test]
    fn decode_normalized_indexed_image_2bpc_expands_samples() {
        let dictionary = indexed_dictionary(2);
        let image =
            decode_normalized_image(&dictionary, vec![0x1B].into(), &PassthroughResolver, None)
                .expect("2-bpc indexed image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            image.data.as_ref(),
            &[
                10, 11, 12, 255, 20, 21, 22, 255, 30, 31, 32, 255, 40, 41, 42, 255,
            ]
        );
    }

    #[test]
    fn decode_normalized_indexed_image_4bpc_expands_samples() {
        let dictionary = indexed_dictionary(4);
        let image = decode_normalized_image(
            &dictionary,
            vec![0x01, 0x23].into(),
            &PassthroughResolver,
            None,
        )
        .expect("4-bpc indexed image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            image.data.as_ref(),
            &[
                10, 11, 12, 255, 20, 21, 22, 255, 30, 31, 32, 255, 40, 41, 42, 255
            ]
        );
    }

    #[test]
    fn decode_normalized_indexed_image_8bpc_continues_to_work() {
        let dictionary = indexed_dictionary(8);
        let image = decode_normalized_image(
            &dictionary,
            vec![0, 1, 2, 3].into(),
            &PassthroughResolver,
            None,
        )
        .expect("8-bpc indexed image should decode");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::RGBA8888);
        assert_eq!(
            image.data.as_ref(),
            &[
                10, 11, 12, 255, 20, 21, 22, 255, 30, 31, 32, 255, 40, 41, 42, 255
            ]
        );
    }

    fn indexed_dictionary(bits_per_component: i64) -> Dictionary {
        Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"BitsPerComponent"),
                ObjectVariant::Integer(bits_per_component),
            ),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Array(vec![
                    ObjectVariant::Name(b"Indexed".to_vec()),
                    ObjectVariant::Name(b"DeviceRGB".to_vec()),
                    ObjectVariant::Integer(3),
                    ObjectVariant::HexString(vec![10, 11, 12, 20, 21, 22, 30, 31, 32, 40, 41, 42]),
                ]),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(4)),
        ]))
    }

    fn filtered_gray_dictionary() -> Dictionary {
        Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (
                Vec::from(b"Filter"),
                ObjectVariant::Name(b"ASCIIHexDecode".to_vec()),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]))
    }
}
