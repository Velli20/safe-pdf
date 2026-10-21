use bytes::Bytes;
use pdf_filter::image_payload::ImagePayload;
use pdf_graphics::Image;

use crate::PdfImage;
use crate::decoded_samples::DecodedSamples;
use crate::error::PdfImageError;
use crate::image_metadata::ImageMetadata;
use crate::raster;
use crate::reduced_image::{BandDecode, PixelBudget, ReduceImage};

/// Limits on the pixels a decoded image is held at.
///
/// Drawing never needs far more pixels than the device shows, and backends cannot
/// create images beyond a few gigabytes or wider than a GPU texture, so a larger
/// image is reduced while it is decoded.
const IMAGE_BUDGET: PixelBudget = PixelBudget {
    pixels: 1 << 25,
    side: 1 << 14,
};

impl PdfImage {
    /// Decodes the image into a raster, applying `soft_mask` as its opacity.
    pub fn decode(&self, soft_mask: Option<&Image>) -> Result<Image, PdfImageError> {
        let (payload, metadata) = (self.payload.clone(), &self.metadata);
        // A non-zero /SMaskInData puts the image's own opacity in charge, and PDF
        // forbids an /SMask beside it, so a stray one is dropped rather than
        // multiplied into alpha that already describes the image.
        let soft_mask = soft_mask.filter(|_| !metadata.smask_in_data.overrides_soft_mask());
        let size = (metadata.size.width(), metadata.size.height());
        let factor = IMAGE_BUDGET.reduction_factor(size.0, size.1);
        if factor > 1 {
            let image = decode_reduced_image(payload, metadata, factor)?;
            return match soft_mask {
                Some(mask) => apply_resized_soft_mask(&image, mask),
                None => Ok(image),
            };
        }
        let decoded_samples = DecodedSamples::decode(payload, metadata)?;
        match soft_mask {
            Some(mask) if (mask.width, mask.height) != size => {
                let image = render_ready_image(decoded_samples, None, metadata);
                apply_resized_soft_mask(&image, mask)
            }
            _ => Ok(render_ready_image(decoded_samples, soft_mask, metadata)),
        }
    }
}

/// Decodes an image reduced by `factor`, without its soft mask.
///
/// Filtered samples are decoded a band of rows at a time. A JPEG 2000 codestream, or
/// samples shorter than the declared rows, are decoded whole and then reduced.
fn decode_reduced_image(
    payload: ImagePayload,
    metadata: &ImageMetadata,
    factor: usize,
) -> Result<Image, PdfImageError> {
    if let ImagePayload::Samples(samples) = &payload
        && let Some(bands) = BandDecode::new(samples, metadata, factor)
    {
        let reduced = bands.decode(|band, band_metadata| {
            let decoded = DecodedSamples::decode(ImagePayload::Samples(band), band_metadata)?;
            if decoded.is_rgba || matches!(decoded.num_color_components, 1 | 3) {
                return Ok(decoded);
            }
            let image = render_ready_image(decoded, None, band_metadata);
            Ok(DecodedSamples {
                num_color_components: 4,
                image_data: image.data,
                is_rgba: true,
            })
        })?;
        return Ok(render_ready_image(reduced, None, &bands.reduced_metadata()));
    }
    let decoded_samples = DecodedSamples::decode(payload, metadata)?;
    render_ready_image(decoded_samples, None, metadata).reduced(factor)
}

/// Builds a render image from decoded samples whose soft mask, if any, has the image's size.
fn render_ready_image(
    decoded_samples: DecodedSamples,
    soft_mask: Option<&Image>,
    metadata: &ImageMetadata,
) -> Image {
    let DecodedSamples {
        num_color_components,
        image_data,
        is_rgba,
    } = decoded_samples;
    if is_rgba {
        return image_from_rgba(
            image_data,
            metadata.size.width(),
            metadata.size.height(),
            soft_mask,
            metadata.interpolate,
        );
    }
    let image = Image::from_decoded_samples(
        image_data,
        metadata.size.width(),
        metadata.size.height(),
        num_color_components,
        soft_mask,
        metadata.interpolate,
    );
    // An image mask's samples are coverage, painted later in the fill color.
    match metadata.image_mask && image.pixel_format == pdf_graphics::PixelFormat::Gray8 {
        true => Image {
            pixel_format: pdf_graphics::PixelFormat::Alpha8,
            ..image
        },
        false => image,
    }
}

/// Most pixels an image combined with a soft mask of a different size is
/// given, unless the image alone already has more.
///
/// A mask can be far finer than the area it is drawn on, and the combined
/// image is held in memory and uploaded for drawing at its full size.
const MAX_COMBINED_PIXELS: usize = 1 << 22;

/// Combines an image with a soft mask of different dimensions.
///
/// Both cover the same unit square, so they are combined at the larger width
/// and the larger height, which keeps the detail of a fine mask over a coarse
/// color image and the reverse. A size beyond [`MAX_COMBINED_PIXELS`] is
/// reduced, with the finer mask averaged down to it.
fn apply_resized_soft_mask(image: &Image, mask: &Image) -> Result<Image, PdfImageError> {
    let (width, height) = combined_size(image, mask);
    Ok(Image {
        data: raster::resample_with_soft_mask(image, mask, width, height)?.into(),
        width,
        height,
        pixel_format: pdf_graphics::PixelFormat::RGBA8888,
        interpolate: image.interpolate,
    })
}

/// Returns the size an image and its differently sized soft mask are combined at.
///
/// That is the larger width and the larger height, divided by the smallest
/// whole factor that brings the pixel count within [`MAX_COMBINED_PIXELS`] or
/// the image's own pixel count, whichever is larger, and each side within
/// [`IMAGE_BUDGET`].
fn combined_size(image: &Image, mask: &Image) -> (usize, usize) {
    let width = image.width.max(mask.width);
    let height = image.height.max(mask.height);
    let budget = PixelBudget {
        pixels: MAX_COMBINED_PIXELS.max(image.width.saturating_mul(image.height)),
        side: IMAGE_BUDGET.side,
    };
    let factor = budget.reduction_factor(width, height);
    (width.div_ceil(factor), height.div_ceil(factor))
}

/// Builds a render image from color-space-converted RGBA and combines its alpha with `/SMask`.
fn image_from_rgba(
    data: Bytes,
    width: usize,
    height: usize,
    soft_mask: Option<&Image>,
    interpolate: bool,
) -> Image {
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
            interpolate,
        };
    }

    // Freshly decoded samples are not shared, so this takes over their buffer without a copy.
    let mut rgba = Vec::from(data);
    rgba.truncate(byte_len);
    if let Some(mask) = soft_mask {
        for ([_, _, _, alpha], mask_alpha) in rgba
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(soft_mask_alphas(mask))
        {
            let combined = u16::from(*alpha).saturating_mul(u16::from(mask_alpha)) / 255;
            *alpha = u8::try_from(combined).unwrap_or(u8::MAX);
        }
    }
    Image {
        data: rgba.into(),
        width,
        height,
        pixel_format: pdf_graphics::PixelFormat::RGBA8888,
        interpolate,
    }
}

/// Samples soft-mask coverage without requiring a complete declared image buffer.
///
/// Gray8 supplies coverage directly; RGBA combines the red channel with alpha.
/// Incomplete RGBA pixels are ignored. The caller zips these values with output
/// pixels, leaving destination alpha unchanged when the mask ends early.
fn soft_mask_alphas(mask: &Image) -> impl Iterator<Item = u8> + '_ {
    raster::rgba_pixels(mask).map(raster::soft_mask_coverage)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use bytes::Bytes;
    use pdf_object_reader::{
        dictionary::Dictionary, object_error::ObjectError, object_resolver::PassthroughResolver,
        object_variant::ObjectVariant, stream::StreamObject,
    };

    use pdf_graphics::{Image, PixelFormat};

    use crate::{PdfImage, error::PdfImageError};

    /// Reads an image XObject stream and decodes it.
    fn decode(stream: StreamObject, soft_mask: Option<&Image>) -> Result<Image, PdfImageError> {
        pdf_object_reader::ObjectReader::new(PassthroughResolver)
            .read::<PdfImage>(&ObjectVariant::Stream(stream))?
            .decode(soft_mask)
    }

    fn name(value: &str) -> ObjectVariant {
        pdf_object_reader::pdf_string::PdfString::from(
            value.as_bytes().to_vec(),
            pdf_object_reader::string_kind::StringKind::Name,
        )
    }

    fn fixture_cal_rgb() -> ObjectVariant {
        ObjectVariant::Array(
            vec![
                name("CalRGB".into()),
                ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([
                    (
                        Vec::from(b"WhitePoint"),
                        ObjectVariant::Array(
                            vec![
                                ObjectVariant::Real(0.9505.into()),
                                ObjectVariant::Real(1.0),
                                ObjectVariant::Real(1.089),
                            ]
                            .into(),
                        ),
                    ),
                    (
                        Vec::from(b"Matrix"),
                        ObjectVariant::Array(
                            vec![
                                ObjectVariant::Real(0.9505.into()),
                                ObjectVariant::Real(1.0),
                                ObjectVariant::Real(1.089),
                                ObjectVariant::Integer(0),
                                ObjectVariant::Integer(0),
                                ObjectVariant::Integer(0),
                                ObjectVariant::Integer(0),
                                ObjectVariant::Integer(0),
                                ObjectVariant::Integer(0),
                            ]
                            .into(),
                        ),
                    ),
                ]))),
            ]
            .into(),
        )
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

        ObjectVariant::Array(
            vec![
                name("DeviceN".into()),
                ObjectVariant::Array(
                    vec![name("IBM".into()), name("None"), name("None"), name("None")].into(),
                ),
                fixture_cal_rgb(),
                ObjectVariant::Stream(function),
            ]
            .into(),
        )
    }

    #[test]
    fn read_xobject_uses_predecoded_filtered_stream_data() {
        let dictionary = filtered_gray_dictionary();
        let stream = StreamObject::new(1, 0, dictionary.clone(), vec![0x2A]);

        let image = decode(stream.clone(), None)
            .expect("predecoded image data should not be filtered again");

        assert_eq!(image.data.as_ptr(), stream.data.as_ptr());
        assert_eq!(image.data.as_ref(), &[0x2A]);
    }

    #[test]
    fn read_xobject_decodes_encoded_stream_data() {
        let dictionary = filtered_gray_dictionary();
        let stream = StreamObject::new_encoded(1, 0, dictionary.clone(), b"2A>".to_vec());

        let image = decode(stream.clone(), None)
            .expect("encoded image data should have its filter applied");

        assert_eq!(image.data.as_ref(), &[0x2A]);
    }

    #[test]
    fn decode_normalized_1bpc_gray_inverts_samples() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(1)),
            (
                Vec::from(b"ColorSpace"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (
                Vec::from(b"Decode"),
                ObjectVariant::Array(
                    vec![ObjectVariant::Integer(1.into()), ObjectVariant::Integer(0)].into(),
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(4)),
        ]));

        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![0b1010_0000]),
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
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (
                Vec::from(b"Decode"),
                ObjectVariant::Array(
                    vec![ObjectVariant::Integer(0.into()), ObjectVariant::Real(0.5)].into(),
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));

        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![0, 255]),
            None,
        )
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
                ObjectVariant::Array(
                    vec![
                        pdf_object_reader::pdf_string::PdfString::from(
                            b"Indexed".to_vec(),
                            pdf_object_reader::string_kind::StringKind::Name,
                        ),
                        pdf_object_reader::pdf_string::PdfString::from(
                            b"DeviceRGB".to_vec(),
                            pdf_object_reader::string_kind::StringKind::Name,
                        ),
                        ObjectVariant::Integer(1),
                        pdf_object_reader::pdf_string::PdfString::from(
                            vec![10, 11, 12, 20, 21, 22],
                            pdf_object_reader::string_kind::StringKind::Hexadecimal,
                        ),
                    ]
                    .into(),
                ),
            ),
            (
                Vec::from(b"Decode"),
                ObjectVariant::Array(
                    vec![ObjectVariant::Integer(1.into()), ObjectVariant::Integer(0)].into(),
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));

        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![0b1000_0000]),
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
                ObjectVariant::Array(
                    vec![
                        name("Indexed".into()),
                        fixture_device_n(),
                        ObjectVariant::Integer(0),
                        pdf_object_reader::pdf_string::PdfString::from(
                            vec![0, 255, 255, 255],
                            pdf_object_reader::string_kind::StringKind::Hexadecimal,
                        ),
                    ]
                    .into(),
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));

        let image = decode(StreamObject::new(1, 0, dictionary.clone(), vec![0]), None)
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

        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![0, 255, 255, 255]),
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
                ObjectVariant::Array(
                    vec![
                        name("Indexed".into()),
                        name("DeviceCMYK"),
                        ObjectVariant::Integer(0),
                        pdf_object_reader::pdf_string::PdfString::from(
                            vec![0, 0, 0, 0],
                            pdf_object_reader::string_kind::StringKind::Hexadecimal,
                        ),
                    ]
                    .into(),
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));

        let image = decode(StreamObject::new(1, 0, dictionary.clone(), vec![0]), None)
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
                    ObjectVariant::Array(
                        vec![ObjectVariant::Integer(0.into()), ObjectVariant::Integer(1)].into(),
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
            b"pop 1 1 1".to_vec(),
        );
        let dictionary = Dictionary::new(BTreeMap::from([
            (Vec::from(b"BitsPerComponent"), ObjectVariant::Integer(8)),
            (
                Vec::from(b"ColorSpace"),
                ObjectVariant::Array(
                    vec![
                        name("Separation".into()),
                        name("None"),
                        name("DeviceRGB"),
                        ObjectVariant::Stream(tint_function),
                    ]
                    .into(),
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));
        let soft_mask = Image {
            data: vec![128].into(),
            width: 1,
            height: 1,
            pixel_format: PixelFormat::Gray8,
            interpolate: false,
        };

        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![255]),
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
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (
                Vec::from(b"Decode"),
                ObjectVariant::Array(vec![ObjectVariant::Integer(0.into())].into()),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));

        let err = decode(StreamObject::new(1, 0, dictionary.clone(), vec![0]), None)
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
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));

        let samples = Bytes::from_static(&[12, 34]);
        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), samples.clone()),
            None,
        )
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
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));
        let samples = Bytes::from_static(&[12, 34, 56]);

        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), samples.clone()),
            None,
        )
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

        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![0b1010_0000]),
            None,
        )
        .expect("image masks should default missing BitsPerComponent to 1");

        assert_eq!(image.pixel_format, pdf_graphics::PixelFormat::Alpha8);
        assert_eq!(image.data.as_ref(), &[0x00, 0xFF, 0x00, 0xFF]);
    }

    #[test]
    fn decode_normalized_non_mask_image_still_requires_bits_per_component() {
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                Vec::from(b"ColorSpace"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]));

        let err = decode(StreamObject::new(1, 0, dictionary.clone(), vec![0]), None)
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
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceCMYK".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (
                Vec::from(b"Filter"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DCTDecode".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));

        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![10, 20, 30, 40, 50, 60]),
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
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceCMYK".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));

        let err = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![10, 20, 30, 40, 50, 60]),
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
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceRGB".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (
                Vec::from(b"Filter"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DCTDecode".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));

        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![0xAA, 0x10, 0x20]),
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
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
        ]));
        let soft_mask = Image {
            width: 2,
            height: 1,
            data: vec![0x10, 0xE0].into(),
            pixel_format: PixelFormat::Gray8,
            interpolate: false,
        };

        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![0x20, 0xC0]),
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
        let image = PdfImage::inline(
            Dictionary::new(BTreeMap::from([
                (Vec::from(b"BPC"), ObjectVariant::Integer(8)),
                (
                    Vec::from(b"CS"),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"DeviceGray".to_vec(),
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                ),
                (
                    Vec::from(b"F"),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"ASCIIHexDecode".to_vec(),
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                ),
                (Vec::from(b"H"), ObjectVariant::Integer(1)),
                (Vec::from(b"W"), ObjectVariant::Integer(1)),
            ])),
            b"2A>".to_vec(),
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        )
        .expect("inline image filters should decode");

        let decoded = image.decode(None).expect("inline image should decode");

        assert_eq!(decoded.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(decoded.data.as_ref(), &[0x2A]);
    }

    #[test]
    fn decode_inline_image_accepts_abbreviated_gray_color_space() {
        let image = PdfImage::inline(
            Dictionary::new(BTreeMap::from([
                (Vec::from(b"BPC"), ObjectVariant::Integer(1)),
                (
                    Vec::from(b"CS"),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"G".to_vec(),
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                ),
                (Vec::from(b"H"), ObjectVariant::Integer(1)),
                (Vec::from(b"W"), ObjectVariant::Integer(4)),
            ])),
            vec![0b1010_0000],
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        )
        .expect("inline image should be constructed");

        let decoded = image
            .decode(None)
            .expect("inline image with abbreviated gray color space should decode");

        assert_eq!(decoded.pixel_format, pdf_graphics::PixelFormat::Gray8);
        assert_eq!(decoded.data.as_ref(), &[0xFF, 0x00, 0xFF, 0x00]);
    }

    #[test]
    fn decode_inline_image_preserves_unfiltered_shared_samples() {
        let image = PdfImage::inline(
            Dictionary::new(BTreeMap::from([
                (Vec::from(b"BPC"), ObjectVariant::Integer(8)),
                (
                    Vec::from(b"CS"),
                    pdf_object_reader::pdf_string::PdfString::from(
                        b"G".to_vec(),
                        pdf_object_reader::string_kind::StringKind::Name,
                    ),
                ),
                (Vec::from(b"H"), ObjectVariant::Integer(1)),
                (Vec::from(b"W"), ObjectVariant::Integer(2)),
            ])),
            vec![12, 34],
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        )
        .expect("inline image should be constructed");
        let samples = image.shared_data();

        let decoded = image
            .decode(None)
            .expect("unfiltered inline image should decode");

        assert_eq!(decoded.data.as_ptr(), samples.as_ptr());
        assert_eq!(decoded.data.as_ref(), &[12, 34]);
    }

    #[test]
    fn decode_inline_image_accepts_abbreviated_indexed_color_space() {
        let image = PdfImage::inline(
            Dictionary::new(BTreeMap::from([
                (Vec::from(b"BPC"), ObjectVariant::Integer(1)),
                (
                    Vec::from(b"CS"),
                    ObjectVariant::Array(
                        vec![
                            pdf_object_reader::pdf_string::PdfString::from(
                                b"I".to_vec(),
                                pdf_object_reader::string_kind::StringKind::Name,
                            ),
                            pdf_object_reader::pdf_string::PdfString::from(
                                b"RGB".to_vec(),
                                pdf_object_reader::string_kind::StringKind::Name,
                            ),
                            ObjectVariant::Integer(1),
                            pdf_object_reader::pdf_string::PdfString::from(
                                vec![10, 11, 12, 20, 21, 22],
                                pdf_object_reader::string_kind::StringKind::Hexadecimal,
                            ),
                        ]
                        .into(),
                    ),
                ),
                (Vec::from(b"H"), ObjectVariant::Integer(1)),
                (Vec::from(b"W"), ObjectVariant::Integer(4)),
            ])),
            vec![0b1010_0000],
            &mut pdf_object_reader::ObjectReader::new(PassthroughResolver).session(),
        )
        .expect("inline image should be constructed");

        let decoded = image
            .decode(None)
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
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(8)),
        ]));

        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![0b1011_0010]),
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
        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![0b1010_0000]),
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
        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![0x1B]),
            None,
        )
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
        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![0x01, 0x23]),
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
        let image = decode(
            StreamObject::new(1, 0, dictionary.clone(), vec![0, 1, 2, 3]),
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
                ObjectVariant::Array(
                    vec![
                        pdf_object_reader::pdf_string::PdfString::from(
                            b"Indexed".to_vec(),
                            pdf_object_reader::string_kind::StringKind::Name,
                        ),
                        pdf_object_reader::pdf_string::PdfString::from(
                            b"DeviceRGB".to_vec(),
                            pdf_object_reader::string_kind::StringKind::Name,
                        ),
                        ObjectVariant::Integer(3),
                        pdf_object_reader::pdf_string::PdfString::from(
                            vec![10, 11, 12, 20, 21, 22, 30, 31, 32, 40, 41, 42],
                            pdf_object_reader::string_kind::StringKind::Hexadecimal,
                        ),
                    ]
                    .into(),
                ),
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
                pdf_object_reader::pdf_string::PdfString::from(
                    b"DeviceGray".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (
                Vec::from(b"Filter"),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"ASCIIHexDecode".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            ),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
            (Vec::from(b"Width"), ObjectVariant::Integer(1)),
        ]))
    }
}
