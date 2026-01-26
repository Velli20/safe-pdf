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

use pdf_graphics::PixelFormat;
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    stream::StreamObject, traits::FromDictionary,
};
use thiserror::Error;

use crate::{
    color_space::{ColorSpace, ColorSpaceError},
    xobject::{XObject, XObjectError, XObjectReader},
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
        #[from]
        source: Box<XObjectError>,
    },
    /// An error occurred while parsing the color space.
    #[error("ColorSpace error: {0}")]
    ColorSpaceError(#[from] ColorSpaceError),
    /// An error occurred while resolving PDF objects.
    #[error("Object error: {0}")]
    ObjectError(#[from] ObjectError),
    #[error("invalid image dimensions: {width}x{height}")]
    InvalidImageDimensions { width: usize, height: usize },
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
#[derive(Debug)]
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

impl XObjectReader for ImageXObject {
    type ErrorType = ImageXObjectError;

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
    fn read_xobject(
        dictionary: &Dictionary,
        stream_data: &StreamObject,
        objects: &ObjectCollection,
    ) -> Result<Self, Self::ErrorType> {
        // Extract required image properties from the dictionary.
        let width = dictionary.get_or_err("Width")?.as_number::<usize>()?;
        let height = dictionary.get_or_err("Height")?.as_number::<usize>()?;
        let bits_per_component = dictionary
            .get_or_err("BitsPerComponent")?
            .as_number::<usize>()?;

        // Parse the optional `/ColorSpace` entry.
        let color_space = ColorSpace::from_dictionary(dictionary, objects)?;

        // Start with a copy of the stream data; we may need to decompress it.
        let raw_data = stream_data.data()?;

        // Determine the number of color components.
        let num_pixels = width.saturating_mul(height);
        let num_color_components = raw_data
            .len()
            .checked_div(num_pixels)
            .ok_or(ImageXObjectError::InvalidImageDimensions { width, height })?;

        // Parse the optional `/SMask` entry and convert to RGBA if needed.
        let smask = Self::parse_smask(dictionary, objects)?;

        let (data, pixel_format) = if smask.is_some() || num_color_components == 3 {
            (
                Self::to_rgba(
                    &raw_data,
                    width,
                    height,
                    num_color_components,
                    smask.as_deref(),
                ),
                PixelFormat::RGBA8888,
            )
        } else {
            (raw_data.into_owned(), PixelFormat::Alpha8)
        };

        Ok(Self {
            width,
            height,
            bits_per_component,
            data,
            pixel_format,
            color_space,
        })
    }
}

impl ImageXObject {
    /// Parses the optional `/SMask` entry for soft mask transparency.
    ///
    /// If present, the SMask must be an Image XObject (typically grayscale)
    /// that provides per-pixel opacity information.
    fn parse_smask(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Option<Box<ImageXObject>>, ImageXObjectError> {
        let Some(smask_obj) = dictionary.get("SMask") else {
            return Ok(None);
        };

        // Resolve the SMask stream reference.
        let stream = smask_obj.try_stream(objects)?;

        // Recursively parse the SMask as an XObject.
        let smask_xobject =
            XObject::read_xobject(&stream.dictionary, stream, objects).map_err(|e| {
                ImageXObjectError::SMaskReadError {
                    source: Box::new(e),
                }
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
    /// - `num_color_components`: Number of color components (1 for gray, 3 for RGB, 4 for RGBA).
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
            // RGBA input: modulate existing alpha by soft mask.
            4 => {
                for (i, chunk) in image_data.chunks_exact(4).take(num_pixels).enumerate() {
                    let &[r, g, b, a] = chunk else { continue };
                    // Modulate alpha: (a * mask) / 255. Result is always <= 255.
                    let mask = get_alpha(i);
                    let alpha = u16::from(a)
                        .saturating_mul(u16::from(mask))
                        .saturating_div(255);

                    let alpha = u8::try_from(alpha).unwrap_or(255);
                    out.extend_from_slice(&[r, g, b, alpha]);
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
            // Fallback for other component counts: extract RGB-like data.
            _ => {
                for i in 0..num_pixels {
                    let base = i.saturating_mul(num_color_components);
                    let r = image_data.get(base).copied().unwrap_or(0);
                    let g = image_data.get(base.saturating_add(1)).copied().unwrap_or(0);
                    let b = image_data.get(base.saturating_add(2)).copied().unwrap_or(0);
                    out.extend_from_slice(&[r, g, b, get_alpha(i)]);
                }
            }
        }
        out
    }
}
