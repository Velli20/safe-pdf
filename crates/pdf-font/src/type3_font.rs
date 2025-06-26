use std::collections::HashMap;

use pdf_content_stream::{error::PdfOperatorError, pdf_operator::PdfOperatorVariant};
use pdf_object::{
    ObjectVariant, dictionary::Dictionary, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

use crate::font_descriptor::FontDescriptorError;

/// Represents a Type 3 font in a PDF document.
///
/// Type 3 fonts are defined by a program that describes the shape of each character.
/// Unlike other font types that rely on predefined glyph descriptions, Type 3 fonts
/// offer more flexibility in defining character shapes, allowing for complex
/// graphical elements within glyphs.  However, they are less efficient and do not
/// support advanced typographic features like hinting.
#[derive(Debug)]
pub struct Type3Font {
    /// A matrix that maps user space coordinates to glyph space coordinates.
    /// It is used to transform glyph outlines during rendering.
    pub font_matrix: [f32; 6],
    /// A procedure defining any special actions to be taken before a character from this font is rendered.
    pub char_procs: HashMap<String, Vec<PdfOperatorVariant>>,
    /// The font's encoding, specifying the mapping from character codes to glyph names.
    pub encoding: Option<FontEncodingDictionary>,
}

/// Defines errors that can occur while parsing a Type 3 font object.
#[derive(Debug, Error, PartialEq)]
pub enum Type3FontError {
    /// A required dictionary entry was missing.
    #[error("Missing required entry '{entry_name}' in Type 3 Font dictionary")]
    MissingEntry { entry_name: &'static str },

    /// A dictionary entry had an unexpected type.
    #[error(
        "Entry '{entry_name}' in Type 3 Font dictionary has invalid type: expected {expected_type}, found {found_type}"
    )]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str,
    },

    #[error("FontDescriptor parsing error: {0}")]
    FontDescriptorError(#[from] FontDescriptorError),

    /// Error converting a PDF value to a number.
    #[error("Failed to convert PDF value to number for '{entry_description}': {err}")]
    NumericConversionError {
        entry_description: &'static str,
        #[source]
        err: pdf_object::error::ObjectError,
    },

    #[error("Failed to resolve /Resources dictionary object reference {obj_num}")]
    FailedResolveResourcesObjectReference { obj_num: i32 },

    /// Failed to resolve an object reference.
    #[error("Failed to resolve object reference {obj_num}")]
    FailedToResolveReference { obj_num: i32 },

    #[error("Encoding dictionary parsing error: {0}")]
    EncodingError(#[from] EncodingError),

    #[error("Error parsing content stream operators: {0}")]
    ContentStreamError(#[from] PdfOperatorError),

    #[error("Duplicate character name '{name}' found in /CharProcs dictionary")]
    DuplicateCharProcName { name: String },

    #[error(
        "The object for character '{name}' in /CharProcs must be a Stream, but found {found_type}"
    )]
    InvalidCharProcObject {
        name: String,
        found_type: &'static str,
    },
}

impl FromDictionary for Type3Font {
    const KEY: &'static str = "Font";
    type ResultType = Self;
    type ErrorType = Type3FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let font_matrix =
            dictionary
                .get_array("FontMatrix")
                .ok_or(Type3FontError::MissingEntry {
                    entry_name: "FontMatrix",
                })?;

        // Assuming FontMatrix is an array of 6 numbers.
        let font_matrix = font_matrix
            .iter()
            .map(|o| o.as_number::<f32>().unwrap_or(0.0))
            .collect::<Vec<f32>>();

        let char_proc_dictionary =
            dictionary
                .get_dictionary("CharProcs")
                .ok_or(Type3FontError::MissingEntry {
                    entry_name: "CharProcs",
                })?;

        // Parse optional /Encoding entry
        let encoding = if let Some(encoding_obj) = dictionary.get("Encoding") {
            match encoding_obj.as_ref() {
                ObjectVariant::Name(name) => {
                    // Named encoding like /StandardEncoding
                    Some(FontEncodingDictionary {
                        base_encoding: Some(name.clone()),
                        differences: HashMap::new(), // No differences specified inline
                    })
                }
                ObjectVariant::Dictionary(dict) => {
                    // Encoding dictionary
                    Some(FontEncodingDictionary::from_dictionary(
                        dict.as_ref(),
                        objects,
                    )?)
                }
                ObjectVariant::Reference(obj_num) => {
                    // Reference to an encoding dictionary
                    let resolved_obj = objects.get(*obj_num).ok_or_else(|| {
                        Type3FontError::FailedToResolveReference { obj_num: *obj_num }
                    })?;
                    match resolved_obj.as_dictionary() {
                        Some(dict) => Some(FontEncodingDictionary::from_dictionary(
                            dict.as_ref(),
                            objects,
                        )?),
                        _ => {
                            return Err(Type3FontError::InvalidEntryType {
                                entry_name: "Encoding (resolved)",
                                expected_type: "Dictionary",
                                found_type: resolved_obj.name(),
                            });
                        }
                    }
                }
                _ => {
                    // Invalid type for /Encoding
                    return Err(Type3FontError::InvalidEntryType {
                        entry_name: "Encoding",
                        expected_type: "Name, Dictionary, or Reference",
                        found_type: encoding_obj.name(),
                    });
                }
            }
        } else {
            // /Encoding is optional
            None
        };

        let mut char_procs = HashMap::new();

        for (name, value) in char_proc_dictionary.dictionary.iter() {
            let Some(number) = value.as_reference() else {
                return Err(Type3FontError::InvalidEntryType {
                    entry_name: "CharProcs",
                    expected_type: "Reference",
                    found_type: value.name(),
                });
            };

            let content_stream_obj = objects
                .get(number)
                .ok_or(Type3FontError::FailedResolveResourcesObjectReference { obj_num: number })?;

            match content_stream_obj {
                ObjectVariant::Stream(stream) => {
                    let operators = PdfOperatorVariant::from(stream.data.as_slice())?;
                    let prev = char_procs.insert(name.to_owned(), operators);
                    if prev.is_some() {
                        return Err(Type3FontError::DuplicateCharProcName {
                            name: name.to_owned(),
                        });
                    }
                }
                other => {
                    return Err(Type3FontError::InvalidCharProcObject {
                        name: name.to_owned(),
                        found_type: other.name(),
                    });
                }
            }
        }

        Ok(Type3Font {
            font_matrix: [
                font_matrix[0],
                font_matrix[1],
                font_matrix[2],
                font_matrix[3],
                font_matrix[4],
                font_matrix[5],
            ],
            char_procs,
            encoding,
        })
    }
}

/// Defines errors that can occur while parsing a font encoding dictionary.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum EncodingError {
    #[error("Invalid entry in /Differences array: expected Integer or Name, found {found_type}")]
    InvalidDifferencesEntryType { found_type: &'static str },
    #[error("Invalid character code in /Differences array: expected 0-255, found {code}")]
    InvalidDifferenceCharCode { code: i64 },
    #[error("Missing required entry '{entry_name}' in Encoding dictionary")]
    MissingEntry { entry_name: &'static str },
}

/// Represents a font encoding dictionary, used to map character codes to glyph names.
#[derive(Debug)]
pub struct FontEncodingDictionary {
    /// The base encoding, which can be a predefined name like `/StandardEncoding`
    /// or `/MacRomanEncoding`.
    pub base_encoding: Option<String>,
    /// A dictionary of differences from the base encoding.
    /// Maps character codes (0-255) to glyph names.
    pub differences: HashMap<u8, String>,
}

impl FromDictionary for FontEncodingDictionary {
    const KEY: &'static str = "Encoding";
    type ResultType = Self;
    type ErrorType = EncodingError;

    fn from_dictionary(
        dictionary: &Dictionary,
        _objects: &ObjectCollection, // No need for objects here based on spec
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let base_encoding = if let Some(base_encoding) = dictionary.get_string("BaseEncoding") {
            Some(base_encoding.to_owned())
        } else {
            None
        };

        let mut differences = HashMap::new();

        if let Some(diff_array) = dictionary.get_array("Differences") {
            let mut current_code = 0;
            let mut iter = diff_array.iter();

            while let Some(entry) = iter.next() {
                match entry {
                    ObjectVariant::Integer(code) => {
                        // This is a 'firstCode'
                        if *code < 0 || *code > 255 {
                            return Err(EncodingError::InvalidDifferenceCharCode { code: *code });
                        }
                        current_code = *code as u8;
                    }
                    ObjectVariant::Name(name) => {
                        // This is a glyph name
                        differences.insert(current_code, name.clone());
                        current_code += 1;
                    }
                    _ => {
                        // Invalid entry type in Differences array
                        return Err(EncodingError::InvalidDifferencesEntryType {
                            found_type: entry.name(),
                        });
                    }
                }
            }
        }

        Ok(FontEncodingDictionary {
            base_encoding,
            differences,
        })
    }
}
