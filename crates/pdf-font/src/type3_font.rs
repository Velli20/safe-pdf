use std::collections::HashMap;

use pdf_content_stream::{error::PdfOperatorError, pdf_operator::PdfOperatorVariant};
use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
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
    #[error("Encoding dictionary parsing error: {0}")]
    EncodingError(#[from] EncodingError),
    #[error("Object error: {0}")]
    ObjectError(#[from] ObjectError),
    #[error("Error parsing content stream operators: {0}")]
    ContentStreamError(#[from] PdfOperatorError),
    #[error("Duplicate character name '{name}' found in /CharProcs dictionary")]
    DuplicateCharProcName { name: String },
}

impl FromDictionary for Type3Font {
    const KEY: &'static str = "Font";
    type ResultType = Self;
    type ErrorType = Type3FontError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let font_matrix = dictionary
            .get_or_err("FontMatrix")?
            .as_array_of::<f32, 6>()?;

        let char_proc_dictionary = dictionary.get_or_err("CharProcs")?.try_dictionary()?;

        // Parse optional `/Encoding` entry
        let encoding = if let Some(encoding_obj) = dictionary.get("Encoding") {
            match objects.resolve_object(encoding_obj)? {
                ObjectVariant::Name(name) => {
                    // Named encoding like /StandardEncoding
                    Some(FontEncodingDictionary {
                        base_encoding: Some(name.clone()),
                        differences: HashMap::new(),
                    })
                }
                ObjectVariant::Dictionary(dict) => {
                    // Encoding dictionary
                    Some(FontEncodingDictionary::from_dictionary(
                        dict.as_ref(),
                        objects,
                    )?)
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
            None
        };

        let mut char_procs = HashMap::new();

        // Iterate over each entry in the `/CharProcs` dictionary.
        // Each entry associates a glyph name with a reference to a content stream object.
        for (name, value) in char_proc_dictionary.dictionary.iter() {
            // Resolve the referenced content stream object from the PDF's object collection.
            // If the reference cannot be resolved, return an error with the object number.
            let content_stream_obj = objects.resolve_stream(value)?;
            // Parse the content stream data into a sequence of PDF operators.
            let operators = PdfOperatorVariant::from(content_stream_obj.data.as_slice())?;
            // Insert the parsed operators into the char_procs map under the glyph name.
            // If a duplicate glyph name is found, return an error to prevent overwriting.
            let prev = char_procs.insert(name.to_owned(), operators);
            if prev.is_some() {
                return Err(Type3FontError::DuplicateCharProcName {
                    name: name.to_owned(),
                });
            }
        }

        Ok(Type3Font {
            font_matrix,
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
    #[error("Invalid type for /Differences: expected Array, found {found_type}")]
    InvalidDifferencesType { found_type: &'static str },
    #[error("Invalid character code in /Differences array: expected 0-255, found {code}")]
    InvalidDifferenceCharCode { code: i64 },
    #[error(
        "Character code overflow in /Differences array while incrementing after code {last_code}"
    )]
    DifferencesCodeOverflow { last_code: u8 },
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
        let base_encoding = dictionary
            .get("BaseEncoding")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut differences = HashMap::new();

        if let Some(diff_array) = dictionary.get("Differences") {
            let diff_array =
                diff_array
                    .as_array()
                    .ok_or_else(|| EncodingError::InvalidDifferencesType {
                        found_type: diff_array.name(),
                    })?;

            let mut current_code: u8 = 0;
            for entry in diff_array.iter() {
                match entry {
                    ObjectVariant::Integer(code) => {
                        let code_i64 = *code;
                        current_code = u8::try_from(code_i64).map_err(|_| {
                            EncodingError::InvalidDifferenceCharCode { code: code_i64 }
                        })?;
                    }
                    ObjectVariant::Name(name) => {
                        differences.insert(current_code, name.clone());
                        current_code = current_code.checked_add(1).ok_or(
                            EncodingError::DifferencesCodeOverflow {
                                last_code: current_code,
                            },
                        )?;
                    }
                    _ => {
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
