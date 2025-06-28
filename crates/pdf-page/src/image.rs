use pdf_object::{
    dictionary::Dictionary, object_collection::ObjectCollection, traits::FromDictionary,
    ObjectVariant,
};
use thiserror::Error;

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
    ResolveError { entry_name: &'static str, obj_num: i32 },
    #[error("SMask must be an Image XObject, but it was not.")]
    SMaskNotImage,
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

impl FromDictionary for ImageXObject {
    const KEY: &'static str = "XObject"; // This is the type for XObjects in general, not just images.
    type ResultType = Self;
    type ErrorType = ImageXObjectError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection, // We assume the stream data is already available
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let width = dictionary
            .get_number("Width")
            .ok_or(ImageXObjectError::MissingEntry {
                entry_name: "Width",
            })? as u32;
        let height = dictionary
            .get_number("Height")
            .ok_or(ImageXObjectError::MissingEntry {
                entry_name: "Height",
            })? as u32;

        // let color_space_name = dictionary
        //     .get_string("ColorSpace")
        //     .ok_or(ImageXObjectError::MissingEntry {
        //         entry_name: "ColorSpace",
        //     })?;
        // let color_space = ColorSpace::from_name(color_space_name);
        // if let ColorSpace::Unsupported(name) = &color_space {
        //     eprintln!("Warning: Unsupported ColorSpace for ImageXObject: {}", name);
        //     // return Err(ImageXObjectError::UnsupportedColorSpace { name: name.clone() });
        // }

        let bits_per_component =
            dictionary
                .get_number("BitsPerComponent")
                .ok_or(ImageXObjectError::MissingEntry {
                    entry_name: "BitsPerComponent",
                })? as u32;

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
                            objects.get(*obj_num).ok_or(ImageXObjectError::ResolveError {
                                entry_name: "SMask",
                                obj_num: *obj_num,
                            })?;

                        match resolved_obj {
                            ObjectVariant::Stream(s) => Some(
                                XObject::from_dictionary_and_stream(
                                    &s.dictionary,
                                    s.data.clone(),
                                    objects,
                                )?,
                            ),
                            _ => {
                                return Err(ImageXObjectError::InvalidEntryType {
                                    entry_name: "SMask",
                                    expected_type: "Stream or Reference to Stream",
                                    found_type: resolved_obj.name(),
                                })
                            }
                        }
                    }
                    ObjectVariant::Stream(s) => Some(XObject::from_dictionary_and_stream(
                        &s.dictionary,
                        s.data.clone(),
                        objects,
                    )?),
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
            data: Vec::new(), // This will be filled by the caller
        })
    }
}

// Define an enum for all XObject types
#[derive(Debug)]
pub enum XObject {
    Image(ImageXObject),
    Form, // Placeholder for Form XObjects
    PS,   // Placeholder for PostScript XObjects
    Unsupported(String),
}

impl XObject {
    pub fn from_dictionary_and_stream(
        dictionary: &Dictionary,
        stream_data: Vec<u8>,
        objects: &ObjectCollection,
    ) -> Result<Self, ImageXObjectError> {
        let subtype = dictionary
            .get_string("Subtype")
            .ok_or(ImageXObjectError::MissingEntry {
                entry_name: "Subtype",
            })?;

        match subtype {
            "Image" => {
                let mut image_xobject = ImageXObject::from_dictionary(dictionary, objects)?;

                image_xobject.data = stream_data;

                Ok(XObject::Image(image_xobject))
            }
            "Form" => Ok(XObject::Form), // TODO: Implement Form XObject parsing
            "PS" => Ok(XObject::PS),     // TODO: Implement PostScript XObject parsing
            _ => Ok(XObject::Unsupported(subtype.to_string())),
        }
    }
}
