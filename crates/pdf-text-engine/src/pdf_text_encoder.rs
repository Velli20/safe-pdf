//! Encoding of Unicode text into the character codes of a PDF font, the inverse of
//! [`crate::pdf_text_decoder`], for drawing text the document does not already contain.

use std::collections::BTreeMap;

use pdf_cmap::{ToUnicodeMap, UnicodeSequence};
use pdf_font::{PdfFontSpec, PdfMetrics, SimpleEncoding};

use crate::{error::TextError, pdf_text_decoder::decode_one_byte_mapping};

/// The character code a font draws each Unicode scalar with.
///
/// It is built from the same decoding layout uses, so each code it produces decodes back
/// to the scalar it encodes.
pub struct PdfTextEncoder {
    /// The lowest one-byte code decoding to each single scalar.
    codes: BTreeMap<char, u8>,
}

impl PdfTextEncoder {
    /// Builds the encoder of `font`.
    ///
    /// A simple or Type 3 font encodes every scalar one of its codes decodes to alone. A
    /// Type 0 font encodes nothing.
    ///
    /// # Errors
    /// Propagates character-code decoding failures.
    pub fn new(font: &PdfFontSpec) -> Result<Self, TextError> {
        let codes = match font {
            PdfFontSpec::Type1(font)
            | PdfFontSpec::MultipleMasterType1(font)
            | PdfFontSpec::TrueType(font) => {
                one_byte_codes(&font.encoding, &font.metrics, font.to_unicode.as_deref())?
            }
            PdfFontSpec::Type3(font) => {
                one_byte_codes(&font.encoding, &font.metrics, font.to_unicode.as_deref())?
            }
            PdfFontSpec::Type0(_) => BTreeMap::new(),
        };
        Ok(Self { codes })
    }

    /// The codes of `text`, leaving out scalars the font has no code for.
    pub fn encode(&self, text: &str) -> Vec<u8> {
        text.chars()
            .filter_map(|character| self.codes.get(&character).copied())
            .collect()
    }
}

/// The lowest one-byte code each single scalar decodes from.
fn one_byte_codes(
    encoding: &SimpleEncoding,
    metrics: &PdfMetrics,
    to_unicode: Option<&ToUnicodeMap>,
) -> Result<BTreeMap<char, u8>, TextError> {
    let mut codes = BTreeMap::new();
    for code in u8::MIN..=u8::MAX {
        let decoded = decode_one_byte_mapping(encoding, metrics, to_unicode, code)?;
        if let UnicodeSequence::Scalar(character) = decoded.unicode {
            codes.entry(character).or_insert(code);
        }
    }
    Ok(codes)
}
