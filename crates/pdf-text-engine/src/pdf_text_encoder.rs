//! Encoding of Unicode text into the character codes of a PDF font, the inverse of
//! [`crate::pdf_text_decoder`], for drawing text the document does not already contain.

use std::{collections::BTreeMap, sync::Arc};

use pdf_cmap::{PdfCMap, PdfCode, ToUnicodeMap, UnicodeSequence};
use pdf_font::{PdfFontSpec, Type0FontSpec};

use crate::{error::TextError, pdf_text_decoder::decode_pdf_text};

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
        match font {
            PdfFontSpec::Type0(font) => Ok(Self::composite(font)),
            simple => Self::one_byte(simple),
        }
    }

    /// The encoder of a simple or Type 3 font: the lowest one-byte code each single
    /// scalar decodes from, and printable ASCII no code decodes to by its own code.
    fn one_byte(font: &PdfFontSpec) -> Result<Self, TextError> {
        let mut codes = BTreeMap::new();
        let bytes: Vec<u8> = (u8::MIN..=u8::MAX).collect();
        decode_pdf_text(font, &bytes, |decoded| -> Result<(), TextError> {
            if let UnicodeSequence::Scalar(character) = decoded.unicode {
                codes.entry(character).or_insert(decoded.source_code);
            }
            Ok(())
        })?;
        for code in PRINTABLE_ASCII {
            codes
                .entry(char::from(code))
                .or_insert(PdfCode::new(u32::from(code), 1)?);
        }
        Ok(Self {
            codes,
            identity: None,
        })
    }

    /// The encoder of a composite font: the codes of its `/ToUnicode` scalars, or its
    /// encoding CMap when that map is the identity.
    fn composite(font: &Type0FontSpec) -> Self {
        let mut codes = BTreeMap::new();
        let identity = match font.to_unicode.as_deref() {
            Some(ToUnicodeMap::CMap(map)) => {
                for (value, unicode) in map.entries() {
                    if let UnicodeSequence::Scalar(character) = unicode
                        && let Some(code) = font.encoding.encode(value)
                    {
                        codes
                            .entry(*character)
                            .and_modify(|lowest: &mut PdfCode| *lowest = (*lowest).min(code))
                            .or_insert(code);
                    }
                }
                None
            }
            Some(ToUnicodeMap::Identity) => Some(Arc::clone(&font.encoding)),
            None => None,
        };
        Self { codes, identity }
    }

    /// The code of `character`, or `None` when the font has none.
    #[must_use]
    pub fn code(&self, character: char) -> Option<PdfCode> {
        self.codes
            .get(&character)
            .copied()
            .or_else(|| self.identity.as_deref()?.encode(u32::from(character)))
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
