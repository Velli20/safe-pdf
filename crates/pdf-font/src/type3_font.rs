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
    /// A rectangle in the glyph coordinate system that encloses all glyphs in the font.
    /// This is used to scale and position the font's glyphs correctly.
    pub font_bounding_box: [f32; 4],
    /// A matrix that maps user space coordinates to glyph space coordinates.
    /// It is used to transform glyph outlines during rendering.
    pub font_matrix: [f32; 6],
    /// A vector of glyph widths, corresponding to entries in `char_procs`.
    /// If present, the number of widths should match the number of character procedures.
    pub widths: Option<Vec<f32>>,
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

    #[error("Invalid FontBBox entry: expected an array of 4 numbers, found array of length {len}")]
    InvalidFontBBoxArrayLength { len: usize },

    #[error("Failed to resolve /Resources dictionary object reference {obj_num}")]
    FailedResolveResourcesObjectReference { obj_num: i32 },

    /// Failed to resolve an object reference.
    #[error("Failed to resolve object reference {obj_num}")]
    FailedToResolveReference { obj_num: i32 },

    #[error("Encoding dictionary parsing error: {0}")]
    EncodingError(#[from] EncodingError),

    #[error("Error parsing content stream operators: {0}")]
    ContentStreamError(#[from] PdfOperatorError),
}

impl FromDictionary for Type3Font {
    const KEY: &'static str = "Font";
    type ResultType = Self;
    type ErrorType = Type3FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let font_bounding_box =
            dictionary
                .get_array("FontBBox")
                .ok_or(Type3FontError::MissingEntry {
                    entry_name: "FontBBox",
                })?;

        let convert_bbox_entry =
            |value: &ObjectVariant, coord_name: &'static str| -> Result<f32, Type3FontError> {
                value
                    .as_number::<f32>()
                    .map_err(|source| Type3FontError::NumericConversionError {
                        entry_description: coord_name,
                        err: source,
                    })
            };

        let font_bounding_box = match font_bounding_box.as_slice() {
            [llx, lly, urx, ury] => [
                convert_bbox_entry(llx, "FontBBox llx")?,
                convert_bbox_entry(lly, "FontBBox lly")?,
                convert_bbox_entry(urx, "FontBBox urx")?,
                convert_bbox_entry(ury, "FontBBox ury")?,
            ],
            arr => {
                return Err(Type3FontError::InvalidFontBBoxArrayLength { len: arr.len() });
            }
        };

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
                    entry_name: "Fixme",
                    expected_type: "number",
                    found_type: value.name(),
                });
            };

            let content_stream_obj = objects
                .get(number)
                .ok_or(Type3FontError::FailedResolveResourcesObjectReference { obj_num: number })?;

            match content_stream_obj {
                ObjectVariant::Stream(stream) => {
                    let prev = char_procs.insert(
                        name.to_owned(),
                        PdfOperatorVariant::from(stream.data.as_slice())?,
                    );
                    if prev.is_some() {
                        panic!()
                    }
                }
                _ => {
                    panic!()
                }
            }
        }

        Ok(Type3Font {
            font_bounding_box,
            font_matrix: [
                font_matrix[0],
                font_matrix[1],
                font_matrix[2],
                font_matrix[3],
                font_matrix[4],
                font_matrix[5],
            ],
            widths: None,
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
///
/// See PDF 1.7 Specification, Section 9.6.6, "Font Encoding Dictionaries".
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
    const KEY: &'static str = "Encoding"; // This is the key in the Font dictionary
    type ResultType = Self;
    type ErrorType = EncodingError; // Use the new error type

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
