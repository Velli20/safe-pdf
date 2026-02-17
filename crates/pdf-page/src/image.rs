//! PDF Image XObject parsing and handling.
//!
//! This module provides types and utilities for parsing PDF Image XObjects,
//! which represent raster images embedded in PDF documents. It handles:
//!
//! - Parsing image metadata (dimensions, color space, bits per component)
//! - Decoding compressed image streams (FlateDecode, DCTDecode)
//! - Processing soft masks (SMask) for transparency
//!
//! # PDF Reference
//!
//! See PDF 32000-1:2008, Section 8.9 "Images" for the full specification.

use std::borrow::Cow;

use pdf_graphics::PixelFormat;
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    stream::StreamObject,
};
use thiserror::Error;

use crate::{
    color_space::{ColorSpace, ColorSpaceError},
    resource_cache::ResourceCache,
    xobject::{XObject, XObjectError},
};

/// Errors that can occur when parsing or processing PDF Image XObjects.
#[derive(Debug, Error)]
pub enum ImageXObjectError {
    /// The `/SMask` entry referenced a non-image XObject.
    ///
    /// Per the PDF specification, soft masks must always be Image XObjects
    /// with a `/ColorSpace` of `/DeviceGray`.
    #[error("SMask must be an Image XObject, but found a different XObject type")]
    SMaskNotImage,
    /// An error occurred while reading the soft mask XObject.
    #[error("Failed to read SMask XObject: {source}")]
    SMaskReadError {
        /// The underlying XObject error.
        source: Box<XObjectError>,
    },
    /// An error occurred while parsing the color space.
    #[error("ColorSpace error: {0}")]
    ColorSpaceError(#[from] ColorSpaceError),
    /// An error occurred while resolving PDF objects.
    #[error("Object error: {0}")]
    ObjectError(#[from] ObjectError),
    /// The image has zero-area dimensions (width or height is zero).
    #[error("image has zero dimensions: {width}x{height}")]
    ZeroImageDimensions { width: usize, height: usize },
    /// The bits per component value is not supported.
    ///
    /// Only 8-bit-per-component images are currently supported.
    #[error("unsupported bits per component: {bits} (only 8 is supported)")]
    UnsupportedBitsPerComponent { bits: usize },
    /// The color space reported zero color components, which is invalid.
    #[error("color space reports zero color components")]
    ZeroColorComponents,
    #[error("failed to decode image: expected at least {expected} bytes, got {actual} bytes")]
    ImageDecodeFailed { expected: usize, actual: usize },
}

/// Represents a PDF Image XObject, which is a self-contained raster image.
///
/// An Image XObject is a type of external object (XObject) used to embed raster images
/// within a PDF document. It consists of a dictionary of metadata and a stream of image data.
/// This struct holds the parsed information from the image's dictionary and its raw data.
///
/// When a soft mask (SMask) is present in the source PDF, the alpha channel is applied
/// to the image data during parsing, producing RGBA output data.
#[derive(Debug, Clone)]
pub struct ImageXObject {
    /// The width of the image in samples (pixels).
    ///
    /// Corresponds to the required `/Width` entry in the image dictionary.
    pub width: usize,
    /// The height of the image in samples (pixels).
    ///
    /// Corresponds to the required `/Height` entry in the image dictionary.
    pub height: usize,
    /// The number of bits used to represent each color component.
    ///
    /// Common values are 1, 2, 4, 8, or 16. For example, a standard RGB image
    /// typically uses 8 bits per component. Corresponds to the `/BitsPerComponent` entry.
    pub bits_per_component: usize,
    /// The raw image stream data (with soft mask alpha applied if present).
    ///
    /// If the original image had a soft mask, this data will be in RGBA format
    /// with the alpha channel already composited. Otherwise, this is the original
    /// sample data which may still be compressed according to the stream filters.
    pub data: Vec<u8>,
    /// The pixel format of the image data.
    pub pixel_format: PixelFormat,
    /// The color space of the image samples.
    ///
    /// Defines how to interpret the sample data as colors. Common color spaces
    /// include DeviceRGB, DeviceGray, and DeviceCMYK. Corresponds to the `/ColorSpace` entry.
    ///
    /// Note: If a soft mask was applied, grayscale images are expanded to RGB
    /// (with alpha), so this field reflects the original color space before expansion.
    pub color_space: Option<ColorSpace>,
}

impl ImageXObject {
    /// Parses an Image XObject from a PDF stream dictionary and data.
    ///
    /// This method extracts all required and optional image properties from the
    /// dictionary, handles filter chains (including two-stage compression), and
    /// recursively processes any soft mask references.
    ///
    /// If a soft mask is present, the alpha channel is applied to the image data
    /// at parse time, producing RGBA output.
    ///
    /// # Arguments
    ///
    /// * `dictionary` - The image's stream dictionary containing metadata.
    /// * `stream_data` - The raw (potentially compressed) image stream bytes.
    /// * `objects` - The document's object collection for resolving references.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Required dictionary entries (`Width`, `Height`, `BitsPerComponent`) are missing
    /// - An unsupported filter or filter combination is encountered
    /// - The color space cannot be parsed
    /// - The soft mask is not a valid Image XObject
    pub fn read_xobject(
        dictionary: &Dictionary,
        stream_data: &StreamObject,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
    ) -> Result<Self, ImageXObjectError> {
        // Extract required image properties from the dictionary.
        let width = dictionary
            .get_or_err("Width")?
            .try_number::<usize>(objects)?;
        let height = dictionary
            .get_or_err("Height")?
            .try_number::<usize>(objects)?;

        if width == 0 || height == 0 {
            return Err(ImageXObjectError::ZeroImageDimensions { width, height });
        }

        let bits_per_component = dictionary
            .get_or_err("BitsPerComponent")?
            .try_number::<usize>(objects)?;

        // Only 8-bit-per-component images are currently supported.
        if bits_per_component != 8 {
            return Err(ImageXObjectError::UnsupportedBitsPerComponent {
                bits: bits_per_component,
            });
        }

        // Parse the optional `/ColorSpace` entry.
        let color_space = ColorSpace::from_dictionary(dictionary, objects)?;

        // Decompress / decode the image stream.
        let raw_data = stream_data.data()?;

        // For Indexed color spaces, expand palette indices to actual color values now
        // and record only the base color space going forward.  Storing the base (not
        // the Indexed wrapper) is critical: downstream rendering code in
        // `resolve_image_data` also checks for Indexed color spaces and would
        // re-expand the data a second time if the Indexed wrapper were kept, producing
        // a buffer that is too small for the declared pixel format and causing Skia to
        // reject the image.
        let (image_data, stored_color_space, num_color_components): (Cow<[u8]>, _, usize) =
            match color_space {
                Some(ColorSpace::Indexed {
                    base,
                    hival,
                    lookup,
                }) => {
                    let base_components = base.num_color_components();
                    let expanded =
                        Self::expand_indexed(raw_data.as_ref(), base_components, hival, &lookup);
                    (Cow::Owned(expanded), Some(*base), base_components)
                }
                other => {
                    let components = match &other {
                        Some(cs) => cs.num_color_components(),
                        None => 1,
                    };
                    (raw_data, other, components)
                }
            };

        if num_color_components == 0 {
            return Err(ImageXObjectError::ZeroColorComponents);
        }

        let num_pixels = width.saturating_mul(height);
        let expected_bytes = num_pixels.saturating_mul(num_color_components);
        if image_data.len() < expected_bytes {
            return Err(ImageXObjectError::ImageDecodeFailed {
                expected: expected_bytes,
                actual: image_data.len(),
            });
        }

        // Parse the optional `/SMask` entry and convert to RGBA if needed.
        let smask = Self::parse_smask(dictionary, objects, cache)?;

        let (data, pixel_format) = if smask.is_some() || num_color_components != 1 {
            // Multi-component or masked images are output as RGBA8888.
            (
                Self::to_rgba(
                    image_data.as_ref(),
                    width,
                    height,
                    num_color_components,
                    smask.as_deref(),
                ),
                PixelFormat::RGBA8888,
            )
        } else {
            // Single-component (grayscale) image without a soft mask.
            (image_data.into_owned(), PixelFormat::Gray8)
        };

        Ok(Self {
            width,
            height,
            bits_per_component,
            data,
            pixel_format,
            color_space: stored_color_space,
        })
    }
}

impl ImageXObject {
    /// Expands an Indexed color space image from palette indices to actual color values.
    ///
    /// Each byte in `data` is a palette index clamped to `0..=hival`. The lookup table
    /// provides `base_components` bytes per index entry.
    fn expand_indexed(data: &[u8], base_components: usize, hival: u8, lookup: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len().saturating_mul(base_components));
        for &index in data {
            let clamped = usize::from(index.min(hival));
            let start = clamped.saturating_mul(base_components);
            let end = start.saturating_add(base_components);
            match lookup.get(start..end) {
                Some(color) => out.extend_from_slice(color),
                None => {
                    // Lookup table shorter than expected; pad with zeros.
                    out.extend(std::iter::repeat_n(0, base_components));
                }
            }
        }
        out
    }

    /// Parses the optional `/SMask` entry for soft mask transparency.
    ///
    /// If present, the SMask must be an Image XObject (typically grayscale)
    /// that provides per-pixel opacity information.
    fn parse_smask(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
    ) -> Result<Option<Box<ImageXObject>>, ImageXObjectError> {
        let Some(smask_obj) = dictionary.get("SMask") else {
            return Ok(None);
        };

        // Resolve the SMask stream reference.
        let stream = smask_obj.try_stream(objects)?;

        // Recursively parse the SMask as an XObject.
        let smask_xobject = XObject::read_xobject(&stream.dictionary, stream, objects, cache)
            .map_err(|e| ImageXObjectError::SMaskReadError {
                source: Box::new(e),
            })?;
        // Ensure the SMask is actually an Image XObject.
        match smask_xobject {
            XObject::Image(img) => Ok(Some(Box::new(img))),
            _ => Err(ImageXObjectError::SMaskNotImage),
        }
    }

    /// Converts image data to RGBA format, optionally applying a soft mask.
    ///
    /// If `smask` is `None`, full opacity (255) is used for the alpha channel.
    /// The soft mask provides per-pixel alpha values when present.
    ///
    /// # Parameters
    ///
    /// - `image_data`: The source image data.
    /// - `width`: Image width in pixels.
    /// - `height`: Image height in pixels.
    /// - `num_color_components`: Number of color components (1 for gray, 3 for RGB, 4 for CMYK).
    /// - `smask`: Optional soft mask image (should be grayscale).
    ///
    /// # Returns
    ///
    /// RGBA image data with alpha channel from the soft mask (or 255 if no mask).
    fn to_rgba(
        image_data: &[u8],
        width: usize,
        height: usize,
        num_color_components: usize,
        smask: Option<&ImageXObject>,
    ) -> Vec<u8> {
        let num_pixels = width.saturating_mul(height);
        let smask_data = smask.map(|s| s.data.as_slice());

        // Helper to get alpha value at index, defaulting to fully opaque.
        let get_alpha =
            |i: usize| -> u8 { smask_data.map_or(255, |data| data.get(i).copied().unwrap_or(255)) };

        let mut out = Vec::with_capacity(num_pixels.saturating_mul(4));

        match num_color_components {
            // CMYK input: convert to RGBA.
            //
            // PDF CMYK components are device values 0–255.
            // R = (255−C)·(255−K)/255, similarly for G and B.
            4 => {
                for (i, chunk) in image_data.chunks_exact(4).take(num_pixels).enumerate() {
                    let &[c, m, y, k] = chunk else { continue };
                    let c_inv = 255u16.saturating_sub(u16::from(c));
                    let m_inv = 255u16.saturating_sub(u16::from(m));
                    let y_inv = 255u16.saturating_sub(u16::from(y));
                    let k_inv = 255u16.saturating_sub(u16::from(k));
                    // Products are at most 255*255=65025, so division by 255 fits in u8.
                    let r = u8::try_from(c_inv.saturating_mul(k_inv) / 255).unwrap_or(0);
                    let g = u8::try_from(m_inv.saturating_mul(k_inv) / 255).unwrap_or(0);
                    let b = u8::try_from(y_inv.saturating_mul(k_inv) / 255).unwrap_or(0);
                    out.extend_from_slice(&[r, g, b, get_alpha(i)]);
                }
            }
            // RGB input: expand to RGBA using soft mask (or 255) as alpha.
            3 => {
                for (i, chunk) in image_data.chunks_exact(3).take(num_pixels).enumerate() {
                    let &[r, g, b] = chunk else { continue };
                    out.extend_from_slice(&[r, g, b, get_alpha(i)]);
                }
            }
            // Grayscale input: expand to RGBA with soft mask as alpha.
            1 => {
                for (i, &gray) in image_data.iter().take(num_pixels).enumerate() {
                    out.extend_from_slice(&[gray, gray, gray, get_alpha(i)]);
                }
            }
            // Fallback for unusual component counts (e.g. 2-channel ICC profiles).
            // Treats the first three available channels as R, G, B per pixel.
            _ => {
                for (i, chunk) in image_data
                    .chunks_exact(num_color_components)
                    .take(num_pixels)
                    .enumerate()
                {
                    let r = chunk.first().copied().unwrap_or(0);
                    let g = chunk.get(1).copied().unwrap_or(0);
                    let b = chunk.get(2).copied().unwrap_or(0);
                    out.extend_from_slice(&[r, g, b, get_alpha(i)]);
                }
            }
        }
        out
    }
}
