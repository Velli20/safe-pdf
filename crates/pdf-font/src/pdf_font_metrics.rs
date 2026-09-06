//! Typed decoding of font dictionaries into horizontal glyph metrics.

use crate::{
    PDF_GLYPH_SPACE_UNITS_PER_EM,
    error::FontError,
    glyph_widths_map::GlyphWidthsMap,
    pdf::{PdfFontDescriptor, PdfGlyphMetric, PdfMetrics},
};
use pdf_object_reader::{
    DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ObjectReadError, ReadResult,
    pdf_array::PdfArray,
};
use std::{collections::BTreeMap, sync::Arc};

impl FromPdfObject for PdfMetrics {
    /// Reads a simple, Type 3, or descendant CID font dictionary.
    ///
    /// The input is the whole font dictionary, since character bounds and
    /// default widths are stored outside the width arrays. Type 0 containers
    /// obtain their metrics from a descendant rather than this decoder.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Self::try_from(&mut context.dictionary()?)
    }
}

impl<A: ObjectAccess + ?Sized> TryFrom<&mut DictionaryContext<'_, A>> for PdfMetrics {
    type Error = ObjectReadError;

    /// Reads metrics within the active font-dictionary traversal.
    ///
    /// Simple fonts default to the descriptor's `/MissingWidth` or 500, Type 3
    /// fonts to zero, and CID fonts to `/DW` or 1000. Missing simple widths and
    /// reversed character ranges leave the explicit table empty. Short arrays
    /// contribute only available widths; surplus values remain unread.
    ///
    /// # Errors
    /// Returns a reader error for missing or unsupported subtypes, malformed
    /// width entries, or invalid required character bounds. CID range errors
    /// retain their font-domain source; optional descriptor hints remain lenient.
    fn try_from(context: &mut DictionaryContext<'_, A>) -> ReadResult<Self> {
        let subtype: Arc<[u8]> = context.required(b"Subtype")?;
        let (default_width, cid_keyed) = match subtype.as_ref() {
            b"Type1" | b"MMType1" | b"TrueType" => {
                // Decode metadata only; font program loading belongs to the font spec.
                let missing_width = context
                    .optional::<PdfFontDescriptor>(b"FontDescriptor")?
                    .and_then(|descriptor| descriptor.missing_width);
                (missing_width.unwrap_or(500.0), false)
            }
            b"Type3" => (0.0, false),
            b"CIDFontType0" | b"CIDFontType2" => (
                context
                    .optional::<f32>(b"DW")?
                    .unwrap_or(PDF_GLYPH_SPACE_UNITS_PER_EM),
                true,
            ),
            other => {
                return Err(FontError::UnsupportedFontSubtype {
                    subtype: String::from_utf8_lossy(other).into_owned(),
                }
                .into());
            }
        };
        let mut metrics = Self {
            default: horizontal_metric(default_width),
            explicit: BTreeMap::new(),
        };
        if cid_keyed {
            if let Some(widths) = context.optional::<GlyphWidthsMap>(GlyphWidthsMap::KEY)? {
                for code in 0_u16..=u16::MAX {
                    if let Some(width) = widths.get_width(code) {
                        metrics
                            .explicit
                            .insert(u32::from(code), horizontal_metric(width));
                    }
                }
            }
        } else if let Some(array) = context.optional::<PdfArray>(b"Widths")? {
            // Standard 14 fonts can omit widths and character bounds entirely.
            // Retain the raw array to avoid decoding surplus entries.
            let first: u16 = context.required(b"FirstChar")?;
            let last: u16 = context.required(b"LastChar")?;
            for (code, value) in (first..=last).zip(array.iter()) {
                let width = context.read(value)?;
                metrics
                    .explicit
                    .insert(u32::from(code), horizontal_metric(width));
            }
        }
        Ok(metrics)
    }
}

/// Constructs the horizontal advance used for PDF glyph width entries.
pub(crate) const fn horizontal_metric(width: f32) -> PdfGlyphMetric {
    PdfGlyphMetric {
        advance_x: width,
        advance_y: 0.0,
        vertical_origin_x: None,
        vertical_origin_y: None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use pdf_object_reader::dictionary::Dictionary;
    use pdf_object_reader::{object_resolver::PassthroughResolver, object_variant::ObjectVariant};
    use std::collections::BTreeMap;

    fn read_metrics(dictionary: &Dictionary) -> ReadResult<PdfMetrics> {
        pdf_object_reader::ObjectReader::new(PassthroughResolver)
            .read::<PdfMetrics>(&ObjectVariant::Dictionary(dictionary.clone()))
    }

    fn make_dict(entries: Vec<(&[u8], ObjectVariant)>) -> Dictionary {
        let map: BTreeMap<Vec<u8>, ObjectVariant> = entries
            .into_iter()
            .map(|(k, v)| (k.to_vec(), v))
            .chain([(
                b"Subtype".to_vec(),
                pdf_object_reader::pdf_string::PdfString::from(
                    b"Type1".to_vec(),
                    pdf_object_reader::string_kind::StringKind::Name,
                ),
            )])
            .collect();
        Dictionary::new(map)
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
        assert_eq!(result.explicit.len(), 3);
        assert_eq!(result.explicit[&32].advance_x, 250.0);
        assert_eq!(result.explicit[&33].advance_x, 300.0);
        assert_eq!(result.explicit[&34].advance_x, 400.0);
    }

    #[test]
    fn missing_widths_returns_default_metrics() {
        let dict = make_dict(vec![(b"FirstChar", int(0)), (b"LastChar", int(0))]);
        let result = read_metrics(&dict).unwrap();
        assert!(result.explicit.is_empty());
        assert_eq!(result.default, horizontal_metric(500.0));
    }

    #[test]
    fn no_widths_no_firstchar_returns_default_metrics() {
        // Standard 14 fonts may have neither /Widths nor /FirstChar.
        let dict = make_dict(vec![]);
        let result = read_metrics(&dict).unwrap();
        assert!(result.explicit.is_empty());
        assert_eq!(result.default, horizontal_metric(500.0));
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
        assert_eq!(result.default, horizontal_metric(500.0));
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
        assert_eq!(result.explicit.len(), 2);
        assert_eq!(result.explicit[&10].advance_x, 100.0);
        assert_eq!(result.explicit[&11].advance_x, 200.0);
        assert!(!result.explicit.contains_key(&12));
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
        assert_eq!(result.explicit.len(), 1);
        assert_eq!(result.explicit[&65].advance_x, 600.0);
    }

    #[test]
    fn single_char_range() {
        let dict = make_dict(vec![
            (b"FirstChar", int(0)),
            (b"LastChar", int(0)),
            (b"Widths", arr(vec![real(1000.0)])),
        ]);
        let result = read_metrics(&dict).unwrap();
        assert_eq!(result.explicit.len(), 1);
        assert_eq!(result.explicit[&0].advance_x, 1000.0);
    }
}
