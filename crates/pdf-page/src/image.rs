use pdf_object::{ObjectVariant, dictionary::Dictionary, object_collection::ObjectCollection};
use thiserror::Error;

use crate::xobject::{XObject, XObjectError, XObjectReader};

#[derive(Debug, Error)]
pub enum ImageXObjectError {
    #[error("Missing required entry '{entry_name}' in Image XObject dictionary")]
    MissingEntry { entry_name: &'static str },
    #[error(
        "Entry '{entry_name}' in Image XObject dictionary has invalid type: expected {expected_type}, found {found_type}"
    )]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str,
    },
    #[error("Failed to convert PDF value to number for '{entry_description}': {source}")]
    NumericConversionError {
        entry_description: &'static str,
        #[source]
        source: pdf_object::error::ObjectError,
    },
    #[error("Unsupported ColorSpace '{name}'")]
    UnsupportedColorSpace { name: String },
    #[error("Unsupported Filter '{name}'")]
    UnsupportedFilter { name: String },
    #[error("Failed to resolve object reference {obj_num} for entry '{entry_name}'")]
    ResolveError {
        entry_name: &'static str,
        obj_num: i32,
    },
    #[error("SMask must be an Image XObject, but it was not.")]
    SMaskNotImage,
    #[error("Error reading Image SMask XObject: {source}")]
    SMaskReadError {
        #[from]
        source: Box<XObjectError>,
    },
}

#[derive(Debug)]
pub enum ColorSpace {
    DeviceGray,
    DeviceRGB,
    DeviceCMYK,
    Indexed,
    // Add more as needed
    Unsupported(String),
}

impl ColorSpace {
    pub fn from_name(name: &str) -> Self {
        match name {
            "DeviceGray" => ColorSpace::DeviceGray,
            "DeviceRGB" => ColorSpace::DeviceRGB,
            "DeviceCMYK" => ColorSpace::DeviceCMYK,
            _ => ColorSpace::Unsupported(name.to_string()),
        }
    }
}

/// Represents the compression filter applied to an image's stream data.
///
/// This corresponds to the `/Filter` entry in a PDF Image XObject's dictionary.
/// The filter specifies the algorithm used to decompress the raw image data.
#[derive(Debug, PartialEq)]
pub enum ImageFilter {
    /// The DCT (Discrete Cosine Transform) filter, used for JPEG-compressed images.
    DCTDecode,
    /// The Flate (zlib/deflate) filter, a lossless compression algorithm.
    FlateDecode,
    /// A filter that is not currently supported.
    Unsupported(String),
}

impl ImageFilter {
    pub fn from_name(name: &str) -> Self {
        match name {
            "DCTDecode" => ImageFilter::DCTDecode,
            "FlateDecode" => ImageFilter::FlateDecode,
            _ => ImageFilter::Unsupported(name.to_string()),
        }
    }
}

/// Represents a PDF Image XObject, which is a self-contained raster image.
///
/// An Image XObject is a type of external object (XObject) used to embed raster images
/// within a PDF document. It consists of a dictionary of metadata and a stream of image data.
/// This struct holds the parsed information from the image's dictionary and its raw data.
#[derive(Debug)]
pub struct ImageXObject {
    /// The width of the image in pixels. Corresponds to the `/Width` entry.
    pub width: u32,
    /// The height of the image in pixels. Corresponds to the `/Height` entry.
    pub height: u32,
    /// The number of bits used to represent each color component.
    /// For example, 8 for a standard RGB image. Corresponds to the `/BitsPerComponent` entry.
    pub bits_per_component: u32,
    /// The filter(s) used to decompress the image data, such as `DCTDecode` (JPEG)
    /// or `FlateDecode`. Corresponds to the `/Filter` entry.
    pub filter: Option<ImageFilter>,
    /// An optional soft mask, which is another `ImageXObject` used for transparency.
    /// Corresponds to the `/SMask` entry.
    pub smask: Option<Box<ImageXObject>>,
    /// The raw, potentially compressed, byte data of the image stream.
    pub data: Vec<u8>,
}

impl XObjectReader for ImageXObject {
    type ErrorType = ImageXObjectError;

    fn read_xobject(
        dictionary: &Dictionary,
        stream_data: &[u8],
        objects: &ObjectCollection,
    ) -> Result<Self, Self::ErrorType> {
        let get_required_u32 = |key: &'static str| -> Result<u32, ImageXObjectError> {
            dictionary
                .get(key)
                .ok_or(ImageXObjectError::MissingEntry { entry_name: key })?
                .as_number::<u32>()
                .map_err(|e| ImageXObjectError::NumericConversionError {
                    entry_description: key,
                    source: e,
                })
        };

        let width = get_required_u32("Width")?;
        let height = get_required_u32("Height")?;
        let bits_per_component = get_required_u32("BitsPerComponent")?;

        let filter = dictionary.get_string("Filter").map(ImageFilter::from_name);
        if let Some(ImageFilter::Unsupported(name)) = &filter {
            eprintln!("Warning: Unsupported Filter for ImageXObject: {}", name);
            // return Err(ImageXObjectError::UnsupportedFilter { name: name.clone() });
        }

        let smask = match dictionary.get("SMask") {
            Some(smask_obj) => {
                let smask_xobject = match smask_obj.as_ref() {
                    ObjectVariant::Reference(obj_num) => {
                        let resolved_obj =
                            objects
                                .get(*obj_num)
                                .ok_or(ImageXObjectError::ResolveError {
                                    entry_name: "SMask",
                                    obj_num: *obj_num,
                                })?;

                        match resolved_obj {
                            ObjectVariant::Stream(s) => Some(
                                XObject::read_xobject(&s.dictionary, s.data.as_slice(), objects)
                                    .map_err(|e| ImageXObjectError::SMaskReadError {
                                        source: Box::new(e),
                                    })?,
                            ),
                            _ => {
                                return Err(ImageXObjectError::InvalidEntryType {
                                    entry_name: "SMask",
                                    expected_type: "Stream or Reference to Stream",
                                    found_type: resolved_obj.name(),
                                });
                            }
                        }
                    }
                    ObjectVariant::Stream(s) => Some(
                        XObject::read_xobject(&s.dictionary, s.data.as_slice(), objects).map_err(
                            |e| ImageXObjectError::SMaskReadError {
                                source: Box::new(e),
                            },
                        )?,
                    ),
                    ObjectVariant::Name(name) if name == "None" => None,
                    other => {
                        return Err(ImageXObjectError::InvalidEntryType {
                            entry_name: "SMask",
                            expected_type: "Stream, Reference, or Name 'None'",
                            found_type: other.name(),
                        });
                    }
                };

                match smask_xobject {
                    Some(XObject::Image(img)) => Some(Box::new(img)),
                    Some(_) => return Err(ImageXObjectError::SMaskNotImage),
                    None => None,
                }
            }
            None => None,
        };

        Ok(Self {
            width,
            height,
            bits_per_component,
            filter,
            smask,
            data: stream_data.to_vec(),
        })
    }
}
