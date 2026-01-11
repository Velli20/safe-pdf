use std::collections::HashMap;

use pdf_content_stream::{error::PdfOperatorError, pdf_operator::PdfOperatorVariant};
use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

use crate::{
    encoding::{Encoding, EncodingReadError, FontEncoding},
    font_descriptor::FontDescriptorError,
};

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
    pub encoding: Option<Encoding>,
}

/// Defines errors that can occur while parsing a Type 3 font object.
#[derive(Debug, Error, PartialEq)]
pub enum Type3FontError {
    #[error("FontDescriptor parsing error: {0}")]
    FontDescriptorError(#[from] FontDescriptorError),
    #[error("Object error: {0}")]
    ObjectError(#[from] ObjectError),
    #[error("Error parsing content stream operators: {0}")]
    ContentStreamError(#[from] PdfOperatorError),
    #[error("Duplicate character name '{name}' found in /CharProcs dictionary")]
    DuplicateCharProcName { name: String },
    #[error("Encoding read error: {0}")]
    EncodingReadError(#[from] EncodingReadError),
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

        // Read optional `/Encoding` entry. This is either a name or a dictionary.
        let encoding = dictionary
            .get("Encoding")
            .map(|enc_obj| {
                let enc_obj = objects.resolve_object(enc_obj)?;
                match enc_obj {
                    ObjectVariant::Dictionary(enc_dictionary) => {
                        Encoding::from_dictionary(enc_dictionary, objects)
                    }
                    _ => Encoding::from_base_encoding(FontEncoding::from(enc_obj.try_str()?)),
                }
            })
            .transpose()?;

        let char_proc_dictionary =
            objects.resolve_dictionary(dictionary.get_or_err("CharProcs")?)?;

        // Iterate over each entry in the `/CharProcs` dictionary.
        // Each entry associates a glyph name with a reference to a content stream object.
        let mut char_procs = HashMap::new();
        for (name, value) in char_proc_dictionary.dictionary.iter() {
            // Resolve the referenced content stream object from the PDF's object collection.
            // If the reference cannot be resolved, return an error with the object number.
            let content_stream_obj = objects.resolve_stream(value)?;

            let data = content_stream_obj.data()?;
            // Parse the content stream data into a sequence of PDF operators.
            let operators = PdfOperatorVariant::from(&data)?;
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
