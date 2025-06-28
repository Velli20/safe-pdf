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

#[derive(Debug, PartialEq)]
pub enum ImageFilter {
    DCTDecode,
    FlateDecode,
    // Add more as needed
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

#[derive(Debug)]
pub struct ImageXObject {
    pub width: u32,
    pub height: u32,
    // pub color_space: ColorSpace,
    pub bits_per_component: u32,
    pub filter: Option<ImageFilter>,
    // pub decode_params: Option<Dictionary>, // TODO: Add support for DecodeParms
    pub smask: Option<Box<ImageXObject>>,
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
