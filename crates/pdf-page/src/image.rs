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

use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    stream::StreamObject, traits::FromDictionary,
};
use thiserror::Error;

use crate::{
    color_space::{ColorSpace, ColorSpaceError},
    image_filter::{ImageFilter, ImageFilterError},
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
    /// An error occurred while parsing image filters.
    #[error("Image filter error: {0}")]
    ImageFilterError(#[from] ImageFilterError),
}

/// Represents a PDF Image XObject, which is a self-contained raster image.
///
/// An Image XObject is a type of external object (XObject) used to embed raster images
/// within a PDF document. It consists of a dictionary of metadata and a stream of image data.
/// This struct holds the parsed information from the image's dictionary and its raw data.
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
    /// The final filter used to decompress the image data.
    ///
    /// After any preprocessing (e.g., FlateDecode for two-stage compression),
    /// this represents the remaining filter to apply. Common filters include
    /// `DCTDecode` (JPEG) and `FlateDecode` (zlib).
    pub filter: Option<ImageFilter>,
    /// An optional soft mask for transparency (alpha channel).
    ///
    /// When present, this is another grayscale `ImageXObject` that defines
    /// per-pixel opacity. Corresponds to the `/SMask` entry.
    pub smask: Option<Box<ImageXObject>>,
    /// The raw image stream data.
    ///
    /// This data may still be compressed according to the [`filter`](Self::filter) field.
    /// If `filter` is `None`, this is raw sample data.
    pub data: Vec<u8>,
    /// The color space of the image samples.
    ///
    /// Defines how to interpret the sample data as colors. Common color spaces
    /// include DeviceRGB, DeviceGray, and DeviceCMYK. Corresponds to the `/ColorSpace` entry.
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
        stream_data: &[u8],
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
        let mut data = stream_data.to_vec();

        // Parse and process the filter chain.
        let filter = match ImageFilter::from_dictionary(dictionary, objects)? {
            Some(filter) => Some(filter.apply(&mut data)?),
            None => None,
        };

        // Parse the optional `/SMask` entry for transparency support.
        let smask = Self::parse_smask(dictionary, objects)?;

        Ok(Self {
            width,
            height,
            bits_per_component,
            filter,
            smask,
            data,
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
        let StreamObject {
            dictionary, data, ..
        } = objects.resolve_stream(smask_obj)?;

        // Recursively parse the SMask as an XObject.
        let smask_xobject =
            XObject::read_xobject(dictionary, data.as_slice(), objects).map_err(|e| {
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
}
