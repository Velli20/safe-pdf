//! Encoding of Unicode text into the character codes of a PDF font, the inverse of
//! [`crate::pdf_text_decoder`], for drawing text the document does not already contain.

use std::{collections::BTreeMap, sync::Arc};

use pdf_cmap::{PdfCMap, PdfCode, ToUnicodeMap, UnicodeSequence};
use pdf_font::{PdfFontSpec, PdfMetrics, SimpleEncoding, Type0FontSpec};

use crate::{error::TextError, pdf_text_decoder::decode_one_byte_mapping};

/// Printable ASCII, which a simple font draws by its own code when no code decodes to it.
const PRINTABLE_ASCII: std::ops::RangeInclusive<u8> = 0x20..=0x7e;

/// The character code a font draws each Unicode scalar with.
///
/// It is built from the same decoding layout uses, so each code it produces decodes back
/// to the scalar it encodes, and it is the one table both drawn text and a host font
/// program choose codes from.
pub struct PdfTextEncoder {
    /// The lowest code decoding to each single scalar.
    codes: BTreeMap<char, PdfCode>,
    /// The encoding CMap of a composite font whose `/ToUnicode` is the identity map, which
    /// encodes each scalar as the code of the same value.
    identity: Option<Arc<dyn PdfCMap>>,
}

impl PdfTextEncoder {
    /// Builds the encoder of `font`.
    ///
    /// A simple or Type 3 font encodes every scalar one of its codes decodes to alone, and
    /// printable ASCII no code decodes to by its own code, as symbolic fonts such as bar
    /// code fonts are drawn. A Type 0 font encodes the scalars its `/ToUnicode` maps.
    ///
    /// # Errors
    /// Propagates character-code decoding failures.
    pub fn new(font: &PdfFontSpec) -> Result<Self, TextError> {
        let (codes, identity) = match font {
            PdfFontSpec::Type1(font)
            | PdfFontSpec::MultipleMasterType1(font)
            | PdfFontSpec::TrueType(font) => (
                one_byte_codes(&font.encoding, &font.metrics, font.to_unicode.as_deref())?,
                None,
            ),
            PdfFontSpec::Type3(font) => (
                one_byte_codes(&font.encoding, &font.metrics, font.to_unicode.as_deref())?,
                None,
            ),
            PdfFontSpec::Type0(font) => composite_codes(font),
        };
        Ok(Self { codes, identity })
    }

    /// The code of `character`, or `None` when the font has none.
    #[must_use]
    pub fn code(&self, character: char) -> Option<PdfCode> {
        self.codes
            .get(&character)
            .copied()
            .or_else(|| composite_code(self.identity.as_deref()?, u32::from(character)))
    }

    /// The codes of `text`, or `None` when the font has no code for one of its scalars.
    #[must_use]
    pub fn encode(&self, text: &str) -> Option<Vec<u8>> {
        let mut bytes = Vec::with_capacity(text.len());
        for character in text.chars() {
            bytes.extend(self.code(character)?.bytes());
        }
        Some(bytes)
    }

    /// Each scalar with the code it is encoded as, in scalar order; scalars of an
    /// identity-mapped composite font are encoded on demand and are not listed.
    pub fn codes(&self) -> impl Iterator<Item = (char, PdfCode)> + '_ {
        self.codes
            .iter()
            .map(|(character, code)| (*character, *code))
    }
}

/// The lowest one-byte code each single scalar decodes from, and printable ASCII no code
/// decodes to by its own code.
fn one_byte_codes(
    encoding: &SimpleEncoding,
    metrics: &PdfMetrics,
    to_unicode: Option<&ToUnicodeMap>,
) -> Result<BTreeMap<char, PdfCode>, TextError> {
    let mut codes = BTreeMap::new();
    for code in u8::MIN..=u8::MAX {
        let decoded = decode_one_byte_mapping(encoding, metrics, to_unicode, code)?;
        if let UnicodeSequence::Scalar(character) = decoded.unicode {
            codes.entry(character).or_insert(decoded.source_code);
        }
    }
    for code in PRINTABLE_ASCII {
        codes
            .entry(char::from(code))
            .or_insert(PdfCode::new(u32::from(code), 1)?);
    }
    Ok(codes)
}

/// The codes of a composite font's `/ToUnicode` scalars, or its encoding CMap when that
/// map is the identity.
fn composite_codes(font: &Type0FontSpec) -> (BTreeMap<char, PdfCode>, Option<Arc<dyn PdfCMap>>) {
    let mut codes = BTreeMap::new();
    match font.to_unicode.as_deref() {
        Some(ToUnicodeMap::CMap(map)) => {
            for (value, unicode) in map.entries() {
                if let UnicodeSequence::Scalar(character) = unicode
                    && let Some(code) = composite_code(font.encoding.as_ref(), value)
                {
                    codes
                        .entry(*character)
                        .and_modify(|lowest: &mut PdfCode| *lowest = (*lowest).min(code))
                        .or_insert(code);
                }
            }
            (codes, None)
        }
        Some(ToUnicodeMap::Identity) => (codes, Some(Arc::clone(&font.encoding))),
        None => (codes, None),
    }
}

/// The shortest code of numeric `value` that `encoding` reads back as exactly that code.
fn composite_code(encoding: &dyn PdfCMap, value: u32) -> Option<PdfCode> {
    (1..=4).find_map(|byte_len| {
        let code = PdfCode::new(value, byte_len).ok()?;
        let bytes: Vec<u8> = code.bytes().collect();
        let mapping = encoding.decode_next(&bytes).ok()??;
        (mapping.source == code).then_some(code)
    })
}
