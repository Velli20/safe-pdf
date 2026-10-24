//! Explicit glyph widths decoded from a font dictionary's `/W` or `/Widths` entry.

use pdf_cmap::{CodeRange, CodeRanges, code_range};
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
    pdf_array::PdfArray,
};
use thiserror::Error;

/// Errors that can occur while reading a `/W` array.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum GlyphWidthsMapError {
    /// The end of a CID range is less than the start.
    #[error("Invalid CID range: c_last ({c_last}) < c_first ({c_first})")]
    InvalidCIDRange {
        /// First CID in the range.
        c_first: u32,
        /// Inclusive final CID in the range.
        c_last: u32,
    },
    /// Missing width value after a CID range.
    #[error("Missing width after c_last for range starting at CID {c_first}")]
    MissingWidthForCIDRange {
        /// First CID of the incomplete range.
        c_first: u32,
    },
    /// CID entry is incomplete.
    #[error("CID {cid} found without a corresponding width array or c_last value")]
    IncompleteCIDEntry {
        /// CID lacking a width array or range end.
        cid: u32,
    },
}

/// Explicit glyph widths of a font, keyed by character code or CID.
///
/// Decoded from a font dictionary: `/W` for CID-keyed fonts (`/CIDFontType0` and
/// `/CIDFontType2`), or `/Widths` indexed from `/FirstChar` for simple and Type 3 fonts.
/// Widths are kept as the ranges the font declares and never expanded per code.
#[derive(Debug, Clone, Default)]
pub struct GlyphWidthsMap {
    /// Width runs, sorted by first code and disjoint.
    runs: Vec<WidthRun>,
}

impl GlyphWidthsMap {
    /// Returns the width the font lists for `code`, if any.
    pub fn get(&self, code: u32) -> Option<f32> {
        let (run, offset) = self.runs.covering(code)?;
        run.width(offset)
    }

    /// Reports whether the font lists no explicit widths.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

impl FromIterator<(u32, f32)> for GlyphWidthsMap {
    /// Collects individual code widths; on duplicate codes the first width wins.
    fn from_iter<I: IntoIterator<Item = (u32, f32)>>(widths: I) -> Self {
        Self {
            runs: code_range::disjoint(
                widths
                    .into_iter()
                    .map(|(code, width)| WidthRun::uniform(code, code, width))
                    .collect(),
            ),
        }
    }
}

impl FromPdfObject for GlyphWidthsMap {
    /// Reads a font dictionary's explicit widths: `/W` for CID-keyed fonts, or `/Widths`
    /// indexed from `/FirstChar` for simple and Type 3 fonts.
    ///
    /// Overlapping `/W` entries resolve to the lower-starting entry.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        let mut context = context.dictionary()?;
        let runs = match context.optional::<CidWidthRuns>(b"W")? {
            Some(CidWidthRuns(runs)) => runs,
            None => context
                .read::<SimpleWidthRun>(object)?
                .0
                .into_iter()
                .collect(),
        };
        Ok(Self {
            runs: code_range::disjoint(runs),
        })
    }
}

/// The runs of a CID font's `/W` array, omitting empty ones.
struct CidWidthRuns(Vec<WidthRun>);

/// Reads each `c_first [w1 ... wn]` or `c_first c_last w` entry of a `/W` array.
impl FromPdfObject for CidWidthRuns {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut items = context.array()?;
        let mut entries = items.array().iter();
        let mut runs = Vec::new();
        while let Some(first) = entries.next() {
            let first = items.read::<u32>(first)?;
            let second = entries
                .next()
                .ok_or(GlyphWidthsMapError::IncompleteCIDEntry { cid: first })?;
            // The second item selects the form; an indirect width array is allowed.
            let second = items.resolve(second)?;
            if let ObjectVariant::Array(_) = second.value() {
                let widths = items.read::<Vec<f32>>(second.value())?;
                if !widths.is_empty() {
                    runs.push(WidthRun::explicit(first, widths.into_boxed_slice()));
                }
                continue;
            }
            let last = items.read::<u32>(second.value())?;
            let width = entries
                .next()
                .ok_or(GlyphWidthsMapError::MissingWidthForCIDRange { c_first: first })?;
            let width = items.read::<f32>(width)?;
            if last < first {
                return Err(GlyphWidthsMapError::InvalidCIDRange {
                    c_first: first,
                    c_last: last,
                }
                .into());
            }
            runs.push(WidthRun::uniform(first, last, width));
        }
        Ok(Self(runs))
    }
}

/// A simple or Type 3 font's `/Widths` as one run starting at `/FirstChar`.
///
/// Fonts without `/Widths`, such as Standard 14 fonts, have no run. Only the
/// `/FirstChar..=/LastChar` widths are decoded, so surplus items stay unread; a reversed
/// or empty range yields no run.
struct SimpleWidthRun(Option<WidthRun>);

impl FromPdfObject for SimpleWidthRun {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let Some(widths) = context.optional::<PdfArray>(b"Widths")? else {
            return Ok(Self(None));
        };
        let first: u16 = context.required(b"FirstChar")?;
        let last: u16 = context.required(b"LastChar")?;
        let count = usize::from(last)
            .checked_sub(usize::from(first))
            .map_or(0, |span| span.saturating_add(1));
        let widths = widths
            .iter()
            .take(count)
            .map(|width| context.read(width))
            .collect::<ReadResult<Box<[f32]>>>()?;
        Ok(Self(
            (!widths.is_empty()).then(|| WidthRun::explicit(u32::from(first), widths)),
        ))
    }
}

/// Widths for the inclusive code range `first..=last`.
#[derive(Debug, Clone)]
struct WidthRun {
    first: u32,
    last: u32,
    widths: RunWidths,
}

/// The widths of one run.
#[derive(Debug, Clone)]
enum RunWidths {
    /// `c_first c_last w`: one width for every code in the run.
    Uniform(f32),
    /// `c_first [w1 ... wn]` or a simple font's `/Widths`: one width per code.
    Explicit(Box<[f32]>),
}

impl WidthRun {
    /// One width shared by every code in `first..=last`.
    fn uniform(first: u32, last: u32, width: f32) -> Self {
        Self {
            first,
            last,
            widths: RunWidths::Uniform(width),
        }
    }

    /// Consecutive widths starting at `first`.
    fn explicit(first: u32, widths: Box<[f32]>) -> Self {
        let span = u32::try_from(widths.len().saturating_sub(1)).unwrap_or(u32::MAX);
        Self {
            first,
            last: first.saturating_add(span),
            widths: RunWidths::Explicit(widths),
        }
    }

    /// Returns the width `offset` codes past the start of the run.
    fn width(&self, offset: u32) -> Option<f32> {
        match &self.widths {
            RunWidths::Uniform(width) => Some(*width),
            RunWidths::Explicit(widths) => widths.get(usize::try_from(offset).ok()?).copied(),
        }
    }
}

impl CodeRange for WidthRun {
    fn first(&self) -> u32 {
        self.first
    }

    fn last(&self) -> u32 {
        self.last
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::error::FontError;
    use pdf_object_reader::{
        EmptySource, object_error::ObjectError, object_variant::ObjectVariant,
    };

    // Wrap a `/W` array in the font dictionary the decoder reads.
    fn font_dictionary(array: Vec<ObjectVariant>) -> ObjectVariant {
        ObjectVariant::Dictionary(pdf_object_reader::dictionary::Dictionary::from_entries([(
            b"W".as_slice(),
            ObjectVariant::Array(array.into()),
        )]))
    }

    // Keep domain-error assertions while exercising the context reader exclusively.
    fn read_widths(array: &[ObjectVariant]) -> Result<GlyphWidthsMap, GlyphWidthsMapError> {
        pdf_object_reader::ObjectReader::new(EmptySource)
            .read::<GlyphWidthsMap>(&font_dictionary(array.to_vec()))
            .map_err(|error| {
                let mut source: &dyn std::error::Error = &error;
                loop {
                    if let Some(FontError::GlyphWidthsMapError(error)) =
                        source.downcast_ref::<FontError>()
                    {
                        return error.clone();
                    }
                    source = source
                        .source()
                        .expect("expected a glyph-width error source");
                }
            })
    }

    // Look up a decoded width by CID.
    fn width(map: &GlyphWidthsMap, cid: u32) -> Option<f32> {
        map.get(cid)
    }

    fn num_i64(n: i64) -> ObjectVariant {
        ObjectVariant::Integer(n)
    }

    // Represent a floating-point width as a PDF real.
    fn num_f32(n: f32) -> ObjectVariant {
        ObjectVariant::Real(f64::from(n))
    }

    // Build a PDF array for a width-entry fixture.
    fn arr(elements: Vec<ObjectVariant>) -> ObjectVariant {
        ObjectVariant::Array(elements.into())
    }

    struct SingleObjectResolver {
        resolved: ObjectVariant,
    }

    impl pdf_object_reader::ObjectSource for SingleObjectResolver {
        type Error = ObjectError;

        fn read_object(
            &self,
            object_id: pdf_object_reader::object_id::ObjectId,
        ) -> Result<Option<pdf_object_reader::pdf_object::PdfObject>, Self::Error> {
            Ok(
                (object_id == pdf_object_reader::object_id::ObjectId::new(42, 0))
                    .then(|| pdf_object_reader::pdf_object::PdfObject::new(self.resolved.clone())),
            )
        }
    }

    #[test]
    fn test_from_array_empty() {
        let input_array = vec![];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert!(glyph_widths_map.is_empty());
    }

    #[test]
    fn test_from_array_single_entry() {
        // [ 0 [500 450] ]
        let input_array = vec![num_i64(0), arr(vec![num_f32(500.0), num_f32(450.0)])];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert_eq!(width(&glyph_widths_map, 0), Some(500.0));
        assert_eq!(width(&glyph_widths_map, 1), Some(450.0));
    }

    #[test]
    fn test_from_array_single_entry_with_indirect_widths_array() {
        // [ 0 Ref(widths) ]
        let widths = arr(vec![num_f32(500.0), num_f32(450.0)]);
        let input_array = vec![
            num_i64(0),
            ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(42, 0)),
        ];
        let resolver = SingleObjectResolver { resolved: widths };

        let glyph_widths_map = pdf_object_reader::ObjectReader::new(resolver)
            .read::<GlyphWidthsMap>(&font_dictionary(input_array))
            .unwrap();

        assert_eq!(width(&glyph_widths_map, 0), Some(500.0));
        assert_eq!(width(&glyph_widths_map, 1), Some(450.0));
    }

    #[test]
    fn test_from_array_allows_redundant_overlap_with_same_width() {
        let input_array = vec![
            num_i64(32),
            arr(vec![num_f32(719.0)]),
            num_i64(0),
            num_i64(180),
            num_f32(719.0),
            num_i64(181),
            arr(vec![num_f32(878.0)]),
            num_i64(182),
            num_i64(65534),
            num_f32(719.0),
        ];

        let glyph_widths_map = read_widths(&input_array).unwrap();

        assert_eq!(width(&glyph_widths_map, 0), Some(719.0));
        assert_eq!(width(&glyph_widths_map, 32), Some(719.0));
        assert_eq!(width(&glyph_widths_map, 181), Some(878.0));
        assert_eq!(width(&glyph_widths_map, 182), Some(719.0));
    }

    #[test]
    fn test_from_array_multiple_entries() {
        // [ 0 [500], 10 [600 650], 20 [700] ]
        let input_array = vec![
            num_i64(0),
            arr(vec![num_f32(500.0)]),
            num_i64(10),
            arr(vec![num_f32(600.0), num_f32(650.0)]),
            num_i64(20),
            arr(vec![num_f32(700.0)]),
        ];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert_eq!(width(&glyph_widths_map, 0), Some(500.0));
        assert_eq!(width(&glyph_widths_map, 10), Some(600.0));
        assert_eq!(width(&glyph_widths_map, 11), Some(650.0));
        assert_eq!(width(&glyph_widths_map, 20), Some(700.0));
    }

    #[test]
    fn test_from_array_missing_widths_array() {
        // [ 0 ] (missing widths array)
        let input_array = vec![num_i64(0)];
        let result = read_widths(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::IncompleteCIDEntry { cid: 0 })
        ));
    }

    #[test]
    fn test_from_array_widths_not_an_array() {
        // [ 0, 500 ] (500 is not an array)
        let input_array = vec![num_i64(0), num_f32(500.0)];
        let result = read_widths(&input_array);
        // Now parsed as start of range with missing width -> MissingWidthForCIDRange
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::MissingWidthForCIDRange { c_first: 0 })
        ));
    }

    #[test]
    fn test_from_array_c_first_c_last_w_form_single_entry() {
        // [ 10 12 600 ] -> CIDs 10, 11, 12 have width 600
        let input_array = vec![num_i64(10), num_i64(12), num_f32(600.0)];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert_eq!(width(&glyph_widths_map, 10), Some(600.0));
        assert_eq!(width(&glyph_widths_map, 11), Some(600.0));
        assert_eq!(width(&glyph_widths_map, 12), Some(600.0));
    }

    #[test]
    fn test_from_array_c_first_c_last_w_form_c_first_equals_c_last() {
        // [ 5 5 300 ] -> CID 5 has width 300
        let input_array = vec![num_i64(5), num_i64(5), num_f32(300.0)];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert_eq!(width(&glyph_widths_map, 5), Some(300.0));
    }

    #[test]
    fn test_from_array_mixed_forms() {
        // [ 0 [500], 10 11 600, 20 [700 750] ]
        let input_array = vec![
            num_i64(0),
            arr(vec![num_f32(500.0)]),
            num_i64(10),
            num_i64(11),
            num_f32(600.0),
            num_i64(20),
            arr(vec![num_f32(700.0), num_f32(750.0)]),
        ];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert_eq!(width(&glyph_widths_map, 0), Some(500.0));
        assert_eq!(width(&glyph_widths_map, 10), Some(600.0));
        assert_eq!(width(&glyph_widths_map, 11), Some(600.0));
        assert_eq!(width(&glyph_widths_map, 20), Some(700.0));
        assert_eq!(width(&glyph_widths_map, 21), Some(750.0));
    }

    #[test]
    fn test_from_array_empty_widths_array_is_noop() {
        // [ 0 [] ]
        let input_array = vec![num_i64(0), arr(vec![])];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert!(glyph_widths_map.is_empty());
        assert_eq!(width(&glyph_widths_map, 0), None);
    }

    #[test]
    fn test_from_array_continues_after_empty_widths_array() {
        // [ 0 [] 1 [500] ]
        let input_array = vec![
            num_i64(0),
            arr(vec![]),
            num_i64(1),
            arr(vec![num_f32(500.0)]),
        ];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert_eq!(width(&glyph_widths_map, 0), None);
        assert_eq!(width(&glyph_widths_map, 1), Some(500.0));
    }

    #[test]
    fn test_from_array_duplicate_start_keeps_first_entry() {
        // [ 0 [500] 0 [600] ]
        let input_array = vec![
            num_i64(0),
            arr(vec![num_f32(500.0)]),
            num_i64(0),
            arr(vec![num_f32(600.0)]),
        ];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert_eq!(width(&glyph_widths_map, 0), Some(500.0));
    }

    #[test]
    fn test_from_array_allows_duplicate_start_with_same_width() {
        // [ 0 [500] 0 [500] ]
        let input_array = vec![
            num_i64(0),
            arr(vec![num_f32(500.0)]),
            num_i64(0),
            arr(vec![num_f32(500.0)]),
        ];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert_eq!(width(&glyph_widths_map, 0), Some(500.0));
    }

    #[test]
    fn test_from_array_overlapping_range_keeps_lower_starting_entry() {
        // [ 0 [500 510] 1 2 600 ] -> the range overlaps the explicit entry and is dropped
        let input_array = vec![
            num_i64(0),
            arr(vec![num_f32(500.0), num_f32(510.0)]),
            num_i64(1),
            num_i64(2),
            num_f32(600.0),
        ];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert_eq!(width(&glyph_widths_map, 1), Some(510.0));
        assert_eq!(width(&glyph_widths_map, 2), None);
    }

    #[test]
    fn test_from_array_overlapping_explicit_keeps_lower_starting_range() {
        // [ 0 2 600 2 [700] ] -> the explicit entry overlaps the range and is dropped
        let input_array = vec![
            num_i64(0),
            num_i64(2),
            num_f32(600.0),
            num_i64(2),
            arr(vec![num_f32(700.0)]),
        ];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert_eq!(width(&glyph_widths_map, 2), Some(600.0));
    }

    #[test]
    fn test_uniform_range_lookup() {
        // [ 10 12 600 ]
        let input_array = vec![num_i64(10), num_i64(12), num_f32(600.0)];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        for cid in 10..=12 {
            assert_eq!(width(&glyph_widths_map, cid), Some(600.0));
        }
        assert_eq!(width(&glyph_widths_map, 13), None);
    }

    #[test]
    fn test_from_array_error_c_last_less_than_c_first() {
        // [ 10 8 600 ]
        let input_array = vec![num_i64(10), num_i64(8), num_f32(600.0)];
        let result = read_widths(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::InvalidCIDRange {
                c_first: 10,
                c_last: 8
            })
        ));
    }

    #[test]
    fn test_from_array_error_missing_w_in_c_first_c_last_w() {
        // [ 10 12 ] (missing w)
        let input_array = vec![num_i64(10), num_i64(12)];
        let result = read_widths(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::MissingWidthForCIDRange { c_first: 10 })
        ));
    }
}
