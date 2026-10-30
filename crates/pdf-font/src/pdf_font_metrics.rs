//! Typed decoding of font dictionaries into glyph widths.

use crate::{
    PDF_GLYPH_SPACE_UNITS_PER_EM,
    error::FontError,
    pdf::{FALLBACK_GLYPH_WIDTH, PdfFontDescriptor, PdfMetrics, SimpleEncoding},
    standard14::Standard14Font,
};
use pdf_object_reader::pdf_name::PdfName;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

impl FromPdfObject for PdfMetrics {
    /// Reads a simple, Type 3, or descendant CID font dictionary.
    ///
    /// The input is the whole font dictionary, since character bounds and
    /// default widths are stored outside the width arrays. Type 0 containers
    /// obtain their metrics from a descendant rather than this decoder.
    ///
    /// Simple fonts default to the descriptor's `/MissingWidth` or
    /// [`FALLBACK_GLYPH_WIDTH`], Type 3 fonts to zero, and CID fonts to `/DW` or
    /// 1000. Missing simple widths and reversed character ranges leave the
    /// explicit table empty. Short arrays contribute only available widths;
    /// surplus values remain unread.
    ///
    /// # Errors
    /// Returns a reader error for missing or unsupported subtypes, malformed
    /// width entries, or invalid required character bounds. CID range errors
    /// retain their font-domain source; optional descriptor hints remain lenient.
    /// The explicit table itself is read by [`crate::glyph_widths_map::GlyphWidthsMap`].
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let value = context.object();
        let mut context = context.dictionary()?;
        let subtype: PdfName = context.required(b"Subtype")?;
        let default_width = match subtype.as_ref() {
            // Decode metadata only; font program loading belongs to the font spec.
            b"Type1" | b"MMType1" | b"TrueType" => context
                .optional::<PdfFontDescriptor>(b"FontDescriptor")?
                .and_then(|descriptor| descriptor.missing_width)
                .unwrap_or(FALLBACK_GLYPH_WIDTH),
            b"Type3" => 0.0,
            b"CIDFontType0" | b"CIDFontType2" => context
                .optional::<f32>(b"DW")?
                .unwrap_or(PDF_GLYPH_SPACE_UNITS_PER_EM),
            other => {
                return Err(FontError::UnsupportedFontSubtype {
                    subtype: String::from_utf8_lossy(other).into_owned(),
                }
                .into());
            }
        };
        Ok(Self {
            default: default_width,
            explicit: context.read(value)?,
        })
    }
}

impl PdfMetrics {
    /// Supplies Standard 14 AFM widths for encoded glyph names when the PDF lists no widths.
    ///
    /// Explicit `/Widths` entries remain authoritative, so a non-empty table is left unchanged.
    pub(crate) fn fill_standard14_widths(
        &mut self,
        font: Standard14Font,
        encoding: &SimpleEncoding,
    ) {
        if self.explicit.is_empty() {
            self.explicit = encoding
                .differences
                .iter()
                .filter_map(|(code, name)| {
                    font.glyph_width(name.0.as_ref())
                        .map(|width| (u32::from(*code), width))
                })
                .collect();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use pdf_object_reader::dictionary::Dictionary;
    use pdf_object_reader::{EmptySource, object_variant::ObjectVariant};
    use std::collections::BTreeMap;

    fn read_metrics(dictionary: &Dictionary) -> ReadResult<PdfMetrics> {
        pdf_object_reader::ObjectReader::new(EmptySource)
            .read::<PdfMetrics>(&ObjectVariant::Dictionary(dictionary.clone()))
    }

    fn make_dict(entries: Vec<(&[u8], ObjectVariant)>) -> Dictionary {
        let map: BTreeMap<Vec<u8>, ObjectVariant> = entries
            .into_iter()
            .map(|(k, v)| (k.to_vec(), v))
            .chain([(
                b"Subtype".to_vec(),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"Type1",
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            )])
            .collect();
        Dictionary::from_iter(map)
    }

    fn int(n: i64) -> ObjectVariant {
        ObjectVariant::Integer(n)
    }

    fn real(n: f64) -> ObjectVariant {
        ObjectVariant::Real(n)
    }

    fn arr(items: Vec<ObjectVariant>) -> ObjectVariant {
        ObjectVariant::Array(items.into())
    }

    #[test]
    fn normal_widths_mapping() {
        let dict = make_dict(vec![
            (b"FirstChar", int(32)),
            (b"LastChar", int(34)),
            (b"Widths", arr(vec![real(250.0), real(300.0), real(400.0)])),
        ]);
        let result = read_metrics(&dict).unwrap();
        assert_eq!(result.explicit.get(32).unwrap(), 250.0);
        assert_eq!(result.explicit.get(33).unwrap(), 300.0);
        assert_eq!(result.explicit.get(34).unwrap(), 400.0);
        assert_eq!(result.explicit.get(35), None);
    }

    #[test]
    fn missing_widths_returns_default_metrics() {
        let dict = make_dict(vec![(b"FirstChar", int(0)), (b"LastChar", int(0))]);
        let result = read_metrics(&dict).unwrap();
        assert!(result.explicit.is_empty());
        assert_eq!(result.default, 500.0);
    }

    #[test]
    fn no_widths_no_firstchar_returns_default_metrics() {
        // Standard 14 fonts may have neither /Widths nor /FirstChar.
        let dict = make_dict(vec![]);
        let result = read_metrics(&dict).unwrap();
        assert!(result.explicit.is_empty());
        assert_eq!(result.default, 500.0);
    }

    #[test]
    fn first_char_greater_than_last_char_returns_empty() {
        let dict = make_dict(vec![
            (b"FirstChar", int(100)),
            (b"LastChar", int(50)),
            (b"Widths", arr(vec![real(500.0)])),
        ]);
        let result = read_metrics(&dict).unwrap();
        assert!(result.explicit.is_empty());
        assert_eq!(result.default, 500.0);
    }

    #[test]
    fn oversized_array_is_truncated() {
        // LastChar - FirstChar + 1 = 2, but array has 5 entries.
        let dict = make_dict(vec![
            (b"FirstChar", int(10)),
            (b"LastChar", int(11)),
            (
                b"Widths",
                arr(vec![
                    real(100.0),
                    real(200.0),
                    real(300.0),
                    real(400.0),
                    real(500.0),
                ]),
            ),
        ]);
        let result = read_metrics(&dict).unwrap();
        assert_eq!(result.explicit.get(10).unwrap(), 100.0);
        assert_eq!(result.explicit.get(11).unwrap(), 200.0);
        assert_eq!(result.explicit.get(12), None);
    }

    #[test]
    fn undersized_array_maps_available_entries() {
        // LastChar - FirstChar + 1 = 3, but array only has 1 entry.
        let dict = make_dict(vec![
            (b"FirstChar", int(65)),
            (b"LastChar", int(67)),
            (b"Widths", arr(vec![real(600.0)])),
        ]);
        let result = read_metrics(&dict).unwrap();
        assert_eq!(result.explicit.get(65).unwrap(), 600.0);
        assert_eq!(result.explicit.get(66), None);
    }

    #[test]
    fn single_char_range() {
        let dict = make_dict(vec![
            (b"FirstChar", int(0)),
            (b"LastChar", int(0)),
            (b"Widths", arr(vec![real(1000.0)])),
        ]);
        let result = read_metrics(&dict).unwrap();
        assert_eq!(result.explicit.get(0).unwrap(), 1000.0);
        assert_eq!(result.explicit.get(1), None);
    }
}
