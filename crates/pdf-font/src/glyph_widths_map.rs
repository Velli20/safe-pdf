//! CID glyph widths decoded from explicit arrays and compact uniform ranges.

use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
};
use std::collections::BTreeMap;
use thiserror::Error;

use crate::error::FontError;

/// Errors that can occur during GlyphWidthsMap parsing from a /W array.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum GlyphWidthsMapError {
    /// The end of a CID range is less than the start.
    #[error("Invalid CID range: c_last ({c_last}) < c_first ({c_first})")]
    InvalidCIDRange {
        /// First CID in the range.
        c_first: u16,
        /// Inclusive final CID in the range.
        c_last: u16,
    },
    /// Missing width value after a CID range.
    #[error("Missing width after c_last for range starting at CID {c_first}")]
    MissingWidthForCIDRange {
        /// First CID of the incomplete range.
        c_first: u16,
    },
    /// CID entry is incomplete.
    #[error("CID {cid} found without a corresponding width array or c_last value")]
    IncompleteCIDEntry {
        /// CID lacking a width array or range end.
        cid: u16,
    },
    /// Duplicate starting CID segment definition.
    #[error("Duplicate CID segment start encountered: {cid}")]
    DuplicateCIDStart {
        /// Starting CID already present in the map.
        cid: u16,
    },
    /// Overlapping CID segment definition.
    #[error("Overlapping CID segment starting at {cid}")]
    OverlappingRange {
        /// CID assigned conflicting widths.
        cid: u16,
    },
    /// Explicit widths array was empty.
    #[error("Empty widths array for starting CID {cid}")]
    EmptyWidthsArray {
        /// Starting CID of the empty internal run.
        cid: u16,
    },
    /// Range length was excessively large (possible malformed file / resource exhaustion risk).
    #[error("Range from {cid} to `u16::MAX` is too large ({length} entries)")]
    RangeTooLarge {
        /// Starting CID of the explicit run.
        cid: u16,
        /// Number of widths in the run.
        length: usize,
    },
}

/// A sequence of explicit widths or a constant width stored with its inclusive end.
#[derive(Debug, Clone, PartialEq)]
enum WidthRun {
    /// Explicit widths: `[c_first [w1 ... wn]]` form.
    Explicit(Vec<f32>),
    /// Uniform width for a continuous range: `[c_first c_last w]` (inclusive end CID).
    Uniform { width: f32, end: u16 },
}

/// Represents a glyph widths map parsed from a PDF `/W` array. This
/// applies to CID-keyed fonts `/CIDFontType0` and `/CIDFontType2`
/// (descendants of /Type0).
#[derive(Default)]
pub struct GlyphWidthsMap {
    /// Ordered mapping from starting CID -> width run segment.
    runs: BTreeMap<u16, WidthRun>,
}

impl GlyphWidthsMap {
    pub const KEY: &'static [u8] = b"W";

    /// Inserts decoded widths, skipping empty arrays and redundant equal-width overlaps.
    fn insert_explicit_widths(
        &mut self,
        cid: u16,
        widths: Vec<f32>,
    ) -> Result<(), GlyphWidthsMapError> {
        if widths.is_empty() {
            return Ok(());
        }
        let length = widths.len();
        if length > usize::from(u16::MAX) {
            // Preserve the explicit-array limit of 65,535 entries.
            return Err(GlyphWidthsMapError::RangeTooLarge { cid, length });
        }
        let mut segment_start = None;
        let mut segment_widths = Vec::new();

        for (offset, width) in widths.iter().enumerate() {
            let offset_u16 = u16::try_from(offset)
                .map_err(|_| GlyphWidthsMapError::RangeTooLarge { cid, length })?;
            let current_cid = cid
                .checked_add(offset_u16)
                .ok_or(GlyphWidthsMapError::RangeTooLarge { cid, length })?;

            if let Some(existing_width) = self.get_width(current_cid) {
                if !widths_match(existing_width, *width) {
                    return Err(GlyphWidthsMapError::OverlappingRange { cid: current_cid });
                }

                if let Some(segment_start) = segment_start.take() {
                    self.insert_explicit_run_no_overlap(
                        segment_start,
                        std::mem::take(&mut segment_widths),
                    )?;
                }
                continue;
            }

            if segment_start.is_none() {
                segment_start = Some(current_cid);
            }
            segment_widths.push(*width);
        }

        if let Some(segment_start) = segment_start {
            self.insert_explicit_run_no_overlap(segment_start, segment_widths)?;
        }

        Ok(())
    }

    /// Insert a uniform run [cid ..= c_last] with constant `width`.
    fn insert_uniform(
        &mut self,
        cid: u16,
        c_last: u16,
        width: f32,
    ) -> Result<(), GlyphWidthsMapError> {
        if c_last < cid {
            return Err(GlyphWidthsMapError::InvalidCIDRange {
                c_first: cid,
                c_last,
            });
        }

        let overlapping_runs = self
            .runs
            .iter()
            .filter_map(|(&start, run)| {
                let end = match run {
                    WidthRun::Explicit(values) => {
                        let len_minus_one = values.len().saturating_sub(1);
                        let span_minus_one = u16::try_from(len_minus_one).ok()?;
                        start.checked_add(span_minus_one)?
                    }
                    WidthRun::Uniform { end, .. } => *end,
                };

                if end < cid || start > c_last {
                    None
                } else {
                    Some((start, end))
                }
            })
            .collect::<Vec<_>>();

        let mut cursor = cid;
        for (start, end) in overlapping_runs {
            if cursor < start {
                self.insert_uniform_run_no_overlap(cursor, start.saturating_sub(1), width)?;
            }

            let overlap_start = cursor.max(start);
            let overlap_end = c_last.min(end);
            for current_cid in overlap_start..=overlap_end {
                let Some(existing_width) = self.get_width(current_cid) else {
                    continue;
                };
                if !widths_match(existing_width, width) {
                    return Err(GlyphWidthsMapError::OverlappingRange { cid: current_cid });
                }
            }

            if overlap_end == u16::MAX {
                cursor = u16::MAX;
                break;
            }
            cursor = overlap_end.saturating_add(1);
        }

        if cursor <= c_last {
            self.insert_uniform_run_no_overlap(cursor, c_last, width)?;
        }

        Ok(())
    }

    /// Stores a nonempty explicit run after validating its bounds and neighbors.
    fn insert_explicit_run_no_overlap(
        &mut self,
        cid: u16,
        widths: Vec<f32>,
    ) -> Result<(), GlyphWidthsMapError> {
        if widths.is_empty() {
            return Err(GlyphWidthsMapError::EmptyWidthsArray { cid });
        }
        if self.runs.contains_key(&cid) {
            return Err(GlyphWidthsMapError::DuplicateCIDStart { cid });
        }
        let length = widths.len();
        let len_minus_one = match length.checked_sub(1) {
            Some(v) => v,
            None => return Err(GlyphWidthsMapError::EmptyWidthsArray { cid }),
        };
        let span_minus_one_u16 = u16::try_from(len_minus_one)
            .map_err(|_| GlyphWidthsMapError::RangeTooLarge { cid, length })?;
        let end = cid
            .checked_add(span_minus_one_u16)
            .ok_or(GlyphWidthsMapError::RangeTooLarge { cid, length })?;
        self.check_overlap(cid, end)?;
        self.runs.insert(cid, WidthRun::Explicit(widths));
        Ok(())
    }

    /// Stores a uniform run after validating its bounds and neighbors.
    fn insert_uniform_run_no_overlap(
        &mut self,
        cid: u16,
        c_last: u16,
        width: f32,
    ) -> Result<(), GlyphWidthsMapError> {
        if self.runs.contains_key(&cid) {
            return Err(GlyphWidthsMapError::DuplicateCIDStart { cid });
        }
        self.check_overlap(cid, c_last)?;
        self.runs
            .insert(cid, WidthRun::Uniform { width, end: c_last });
        Ok(())
    }

    /// Ensures new run [start, end] does not intersect any existing run.
    fn check_overlap(&self, start: u16, end: u16) -> Result<(), GlyphWidthsMapError> {
        if start > end {
            return Err(GlyphWidthsMapError::InvalidCIDRange {
                c_first: start,
                c_last: end,
            });
        }
        if let Some((&prev_start, prev_run)) = self.runs.range(..start).next_back() {
            let prev_end = match prev_run {
                WidthRun::Explicit(v) => {
                    if v.is_empty() {
                        prev_start
                    } else {
                        let len_minus_one = v.len().saturating_sub(1);
                        let span_minus_one = u16::try_from(len_minus_one).unwrap_or(u16::MAX);
                        prev_start.saturating_add(span_minus_one)
                    }
                }
                WidthRun::Uniform { end, .. } => *end,
            };
            if prev_end >= start {
                return Err(GlyphWidthsMapError::OverlappingRange { cid: start });
            }
        }
        if let Some((&next_start, _)) = self.runs.range(start.saturating_add(1)..).next()
            && next_start <= end
        {
            return Err(GlyphWidthsMapError::OverlappingRange { cid: start });
        }
        Ok(())
    }

    /// Returns the width for a given CID (character ID), if present.
    ///
    /// # Arguments
    ///
    /// - `character_id` - The CID to look up.
    ///
    /// # Returns
    ///
    /// `Some(width)` if the width is found, or `None` if not present.
    pub(crate) fn get_width(&self, character_id: u16) -> Option<f32> {
        let (start, run) = self.runs.range(..=character_id).next_back()?;
        let offset = character_id.checked_sub(*start)?;
        match run {
            WidthRun::Explicit(widths) => widths.get(usize::from(offset)).copied(),
            WidthRun::Uniform { width, end } => (character_id <= *end).then_some(*width),
        }
    }
}

/// Allows redundant widths that differ by no more than floating-point epsilon.
fn widths_match(left: f32, right: f32) -> bool {
    (left - right).abs() <= f32::EPSILON
}

/// The second item of a CID-width entry determines its array or range form.
enum WidthEntry {
    /// Widths for consecutive CIDs starting at the entry's first CID.
    Explicit(Vec<f32>),
    /// Inclusive final CID, followed by one width in the enclosing array.
    End(u16),
}

impl FromPdfObject for WidthEntry {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        if matches!(context.object().value(), ObjectVariant::Array(_)) {
            Vec::<f32>::from_pdf_object(context).map(Self::Explicit)
        } else {
            u16::from_pdf_object(context).map(Self::End)
        }
    }
}

impl FromPdfObject for GlyphWidthsMap {
    /// Reads `/W` entries as `c_first [w1 ... wn]` or `c_first c_last w`.
    ///
    /// Indexed reads resolve indirect values and retain array error locations.
    /// Empty explicit arrays are ignored; overlapping entries must agree on widths.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.array()?;
        let mut map = Self::default();
        let mut index = 0_usize;
        while index < context.array().len() {
            let cid = context.at::<u16>(index)?;
            index = index.saturating_add(1);
            if index == context.array().len() {
                return Err(
                    FontError::from(GlyphWidthsMapError::IncompleteCIDEntry { cid }).into(),
                );
            }
            match context.at::<WidthEntry>(index)? {
                WidthEntry::Explicit(widths) => {
                    map.insert_explicit_widths(cid, widths)
                        .map_err(FontError::from)?;
                    index = index.saturating_add(1);
                }
                WidthEntry::End(end) => {
                    index = index.saturating_add(1);
                    if index == context.array().len() {
                        return Err(FontError::from(
                            GlyphWidthsMapError::MissingWidthForCIDRange { c_first: cid },
                        )
                        .into());
                    }
                    let width = context.at(index)?;
                    map.insert_uniform(cid, end, width)
                        .map_err(FontError::from)?;
                    index = index.saturating_add(1);
                }
            }
        }
        Ok(map)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use pdf_object_reader::{
        object_error::ObjectError, object_resolver::ObjectResolver,
        object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    // Keep domain-error assertions while exercising the context reader exclusively.
    fn read_widths(array: &[ObjectVariant]) -> Result<GlyphWidthsMap, GlyphWidthsMapError> {
        pdf_object_reader::ObjectReader::new(PassthroughResolver)
            .read::<GlyphWidthsMap>(&ObjectVariant::Array(array.to_vec().into()))
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

    impl ObjectResolver for SingleObjectResolver {
        fn resolve_object<'a>(
            &'a self,
            obj: &'a ObjectVariant,
        ) -> Result<&'a ObjectVariant, ObjectError> {
            match obj {
                ObjectVariant::Reference(_) => Ok(&self.resolved),
                _ => Ok(obj),
            }
        }
    }

    #[test]
    fn test_from_array_empty() {
        let input_array = vec![];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert!(glyph_widths_map.runs.is_empty());
    }

    #[test]
    fn test_from_array_single_entry() {
        // [ 0 [500 450] ]
        let input_array = vec![num_i64(0), arr(vec![num_f32(500.0), num_f32(450.0)])];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert_eq!(glyph_widths_map.runs.len(), 1);
        assert_eq!(glyph_widths_map.get_width(0), Some(500.0));
        assert_eq!(glyph_widths_map.get_width(1), Some(450.0));
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
            .read::<GlyphWidthsMap>(&ObjectVariant::Array(input_array.into()))
            .unwrap();

        assert_eq!(glyph_widths_map.runs.len(), 1);
        assert_eq!(glyph_widths_map.get_width(0), Some(500.0));
        assert_eq!(glyph_widths_map.get_width(1), Some(450.0));
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

        assert_eq!(glyph_widths_map.get_width(0), Some(719.0));
        assert_eq!(glyph_widths_map.get_width(32), Some(719.0));
        assert_eq!(glyph_widths_map.get_width(181), Some(878.0));
        assert_eq!(glyph_widths_map.get_width(182), Some(719.0));
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
        assert_eq!(glyph_widths_map.runs.len(), 3);
        assert_eq!(glyph_widths_map.get_width(0), Some(500.0));
        assert_eq!(glyph_widths_map.get_width(10), Some(600.0));
        assert_eq!(glyph_widths_map.get_width(11), Some(650.0));
        assert_eq!(glyph_widths_map.get_width(20), Some(700.0));
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
    fn test_get_width_empty_map() {
        let glyph_widths_map = GlyphWidthsMap::default();
        assert_eq!(glyph_widths_map.get_width(0), None);
    }

    #[test]
    fn test_get_width_exact_match_start_cid() {
        let mut runs = BTreeMap::new();
        runs.insert(10, WidthRun::Explicit(vec![500.0, 550.0]));
        let glyph_widths_map = GlyphWidthsMap { runs };
        assert_eq!(glyph_widths_map.get_width(10), Some(500.0));
    }

    #[test]
    fn test_get_width_within_range() {
        let mut runs = BTreeMap::new();
        runs.insert(10, WidthRun::Explicit(vec![500.0, 550.0, 600.0]));
        let glyph_widths_map = GlyphWidthsMap { runs };
        assert_eq!(glyph_widths_map.get_width(11), Some(550.0));
    }

    #[test]
    fn test_get_width_end_of_range() {
        let mut runs = BTreeMap::new();
        runs.insert(10, WidthRun::Explicit(vec![500.0, 550.0, 600.0]));
        let glyph_widths_map = GlyphWidthsMap { runs };
        assert_eq!(glyph_widths_map.get_width(12), Some(600.0));
    }

    #[test]
    fn test_get_width_cid_before_range() {
        let mut runs = BTreeMap::new();
        runs.insert(10, WidthRun::Explicit(vec![500.0]));
        let glyph_widths_map = GlyphWidthsMap { runs };
        assert_eq!(glyph_widths_map.get_width(9), None);
    }

    #[test]
    fn test_get_width_cid_after_range() {
        let mut runs = BTreeMap::new();
        runs.insert(10, WidthRun::Explicit(vec![500.0, 550.0]));
        let glyph_widths_map = GlyphWidthsMap { runs };
        assert_eq!(glyph_widths_map.get_width(12), None);
    }

    #[test]
    fn test_get_width_cid_between_ranges() {
        let mut runs = BTreeMap::new();
        runs.insert(0, WidthRun::Explicit(vec![100.0, 110.0]));
        runs.insert(10, WidthRun::Explicit(vec![500.0, 550.0]));
        let glyph_widths_map = GlyphWidthsMap { runs };
        assert_eq!(glyph_widths_map.get_width(5), None); // Between ranges
        assert_eq!(glyph_widths_map.get_width(0), Some(100.0));
        assert_eq!(glyph_widths_map.get_width(1), Some(110.0));
        assert_eq!(glyph_widths_map.get_width(10), Some(500.0));
        assert_eq!(glyph_widths_map.get_width(11), Some(550.0));
    }

    #[test]
    fn test_get_width_multiple_ranges_correct_selection() {
        let mut runs = BTreeMap::new();
        runs.insert(100, WidthRun::Explicit(vec![1000.0]));
        runs.insert(0, WidthRun::Explicit(vec![100.0, 110.0, 120.0]));
        runs.insert(50, WidthRun::Explicit(vec![500.0, 510.0]));
        let glyph_widths_map = GlyphWidthsMap { runs };

        assert_eq!(glyph_widths_map.get_width(1), Some(110.0));
        assert_eq!(glyph_widths_map.get_width(50), Some(500.0));
        assert_eq!(glyph_widths_map.get_width(51), Some(510.0));
        assert_eq!(glyph_widths_map.get_width(100), Some(1000.0));
        assert_eq!(glyph_widths_map.get_width(3), None); // After first range
        assert_eq!(glyph_widths_map.get_width(52), None); // After second range
    }

    #[test]
    fn test_from_array_c_first_c_last_w_form_single_entry() {
        // [ 10 12 600 ] -> CIDs 10, 11, 12 have width 600
        let input_array = vec![num_i64(10), num_i64(12), num_f32(600.0)];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert_eq!(glyph_widths_map.runs.len(), 1);
        assert_eq!(glyph_widths_map.get_width(10), Some(600.0));
        assert_eq!(glyph_widths_map.get_width(11), Some(600.0));
        assert_eq!(glyph_widths_map.get_width(12), Some(600.0));
    }

    #[test]
    fn test_from_array_c_first_c_last_w_form_c_first_equals_c_last() {
        // [ 5 5 300 ] -> CID 5 has width 300
        let input_array = vec![num_i64(5), num_i64(5), num_f32(300.0)];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert_eq!(glyph_widths_map.runs.len(), 1);
        assert_eq!(glyph_widths_map.get_width(5), Some(300.0));
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
        assert_eq!(glyph_widths_map.runs.len(), 3);
        assert_eq!(glyph_widths_map.get_width(0), Some(500.0));
        assert_eq!(glyph_widths_map.get_width(10), Some(600.0));
        assert_eq!(glyph_widths_map.get_width(11), Some(600.0));
        assert_eq!(glyph_widths_map.get_width(20), Some(700.0));
        assert_eq!(glyph_widths_map.get_width(21), Some(750.0));
    }

    #[test]
    fn test_from_array_empty_widths_array_is_noop() {
        // [ 0 [] ]
        let input_array = vec![num_i64(0), arr(vec![])];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        assert!(glyph_widths_map.runs.is_empty());
        assert_eq!(glyph_widths_map.get_width(0), None);
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
        assert_eq!(glyph_widths_map.runs.len(), 1);
        assert_eq!(glyph_widths_map.get_width(0), None);
        assert_eq!(glyph_widths_map.get_width(1), Some(500.0));
    }

    #[test]
    fn test_from_array_error_duplicate_start() {
        // [ 0 [500] 0 [600] ]
        let input_array = vec![
            num_i64(0),
            arr(vec![num_f32(500.0)]),
            num_i64(0),
            arr(vec![num_f32(600.0)]),
        ];
        let result = read_widths(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::OverlappingRange { cid: 0 })
        ));
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
        assert_eq!(glyph_widths_map.get_width(0), Some(500.0));
    }

    #[test]
    fn test_from_array_error_overlapping_range_with_explicit() {
        // [ 0 [500 510] 1 2 600 ] -> second segment overlaps (explicit covers 0,1)
        let input_array = vec![
            num_i64(0),
            arr(vec![num_f32(500.0), num_f32(510.0)]),
            num_i64(1),
            num_i64(2),
            num_f32(600.0),
        ];
        let result = read_widths(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::OverlappingRange { cid: 1 })
        ));
    }

    #[test]
    fn test_from_array_error_overlapping_explicit_with_range() {
        // [ 0 2 600 2 [700] ] -> explicit start 2 overlaps (range covers 0,1,2)
        let input_array = vec![
            num_i64(0),
            num_i64(2),
            num_f32(600.0),
            num_i64(2),
            arr(vec![num_f32(700.0)]),
        ];
        let result = read_widths(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::OverlappingRange { cid: 2 })
        ));
    }

    #[test]
    fn test_uniform_range_lookup() {
        // [ 10 12 600 ]
        let input_array = vec![num_i64(10), num_i64(12), num_f32(600.0)];
        let glyph_widths_map = read_widths(&input_array).unwrap();
        for cid in 10..=12 {
            assert_eq!(glyph_widths_map.get_width(cid), Some(600.0));
        }
        assert_eq!(glyph_widths_map.get_width(13), None);
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
