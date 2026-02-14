use std::collections::HashMap;

use pdf_content_stream::{error::PdfOperatorError, pdf_operator::PdfOperatorVariant};
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};
use thiserror::Error;

use crate::encoding::{Encoding, EncodingReadError, FontEncoding};

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
    #[error("Object error: {0}")]
    ObjectError(#[from] ObjectError),
    #[error("Error parsing content stream operators: {0}")]
    ContentStreamError(#[from] PdfOperatorError),
    #[error("Duplicate character name '{name}' found in /CharProcs dictionary")]
    DuplicateCharProcName { name: String },
    #[error("Encoding read error: {0}")]
    EncodingReadError(#[from] EncodingReadError),
}

impl Type3Font {
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, Type3FontError> {
        let font_matrix = dictionary
            .get_or_err("FontMatrix")?
            .try_array_of::<f32, 6>(objects)?;

        // Read optional `/Encoding` entry. This is either a name or a dictionary.
        let encoding = dictionary
            .get("Encoding")
            .map(|enc_obj| match objects.resolve_object(enc_obj)? {
                ObjectVariant::Dictionary(enc_dictionary) => {
                    Encoding::from_dictionary(enc_dictionary, objects)
                }
                other => Encoding::from_base_encoding(FontEncoding::from(other.try_str(objects)?)),
            })
            .transpose()?;

        let char_proc_dictionary = dictionary
            .get_or_err("CharProcs")?
            .try_dictionary(objects)?;

        // Iterate over each entry in the `/CharProcs` dictionary.
        // Each entry associates a glyph name with a reference to a content stream object.
        let mut char_procs = HashMap::new();
        for (name, value) in char_proc_dictionary.dictionary.iter() {
            // Resolve the content stream data.
            let data = value.try_stream(objects)?.data()?;

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
