use pdf_object::{ObjectVariant, error::ObjectError};
use std::collections::BTreeMap;
use thiserror::Error;

/// Errors that can occur during GlyphWidthsMap parsing from a /W array.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum GlyphWidthsMapError {
    /// The end of a CID range is less than the start.
    #[error("Invalid CID range: c_last ({c_last}) < c_first ({c_first})")]
    InvalidCIDRange { c_first: i64, c_last: i64 },
    /// Missing width value after a CID range.
    #[error("Missing width after c_last for range starting at CID {c_first}")]
    MissingWidthForCIDRange { c_first: i64 },
    /// Unexpected element type after a CID.
    #[error("Expected array or c_last after CID {cid}, found {found_type}")]
    UnexpectedElementAfterCID { cid: i64, found_type: &'static str },
    /// CID entry is incomplete.
    #[error("CID {cid} found without a corresponding width array or c_last value")]
    IncompleteCIDEntry { cid: i64 },
    /// Negative CIDs are not valid according to the specification.
    #[error("Negative CID encountered: {cid}")]
    NegativeCID { cid: i64 },
    /// Duplicate starting CID segment definition.
    #[error("Duplicate CID segment start encountered: {cid}")]
    DuplicateCIDStart { cid: i64 },
    /// Overlapping CID segment definition.
    #[error("Overlapping CID segment starting at {cid}")]
    OverlappingRange { cid: i64 },
    /// Explicit widths array was empty.
    #[error("Empty widths array for starting CID {cid}")]
    EmptyWidthsArray { cid: i64 },
    /// Range length was excessively large (possible malformed file / resource exhaustion risk).
    #[error("Range from {c_first} to {c_last} is too large ({length} entries)")]
    RangeTooLarge {
        c_first: i64,
        c_last: i64,
        length: u64,
    },
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
}

/// Stores glyph widths for CIDs, parsed from the /W array.
///
/// The internal map uses the starting CID as the key and a vector of
/// widths as the value. For a range, the vector contains repeated
/// widths; for an explicit array, the vector contains the widths as specified.
/// Represents either an explicit sequence of widths or a uniform run.
#[derive(Debug, Clone, PartialEq)]
enum WidthRun {
    /// Explicit widths: `[c_first [w1 ... wn]]` form.
    Explicit(Vec<f32>),
    /// Uniform width for a continuous range: `[c_first c_last w]` form.
    Uniform { width: f32, len: u32 },
}

pub struct GlyphWidthsMap {
    /// Ordered mapping from starting CID -> width run segment.
    runs: BTreeMap<i64, WidthRun>,
}

impl GlyphWidthsMap {
    /// Parses a PDF /W array into a `GlyphWidthsMap`.
    ///
    /// The /W array can contain entries of the form:
    /// - `[c_first [w1 ... wn]]` (explicit widths for consecutive CIDs starting at `c_first`)
    /// - `[c_first c_last w]` (all CIDs from `c_first` to `c_last` have width `w`)
    ///
    /// # Arguments
    ///
    /// - `array`: - The PDF array representing the /W widths.
    ///
    /// # Errors
    ///
    /// Returns `GlyphWidthsMapError` if the array is malformed or contains invalid values.
    pub fn from_array(array: &[ObjectVariant]) -> Result<Self, GlyphWidthsMapError> {
        // High-level parser: iterates the raw /W array and delegates to helper
        // routines to insert explicit or uniform width runs. The logic here is
        // intentionally minimalist; validation & overlap checks live in helpers.
        let mut map = GlyphWidthsMap {
            runs: BTreeMap::new(),
        };
        let mut i = 0usize;
        while i < array.len() {
            let cid = array[i].as_number::<i64>()?;
            if cid < 0 {
                return Err(GlyphWidthsMapError::NegativeCID { cid });
            }
            i = i.saturating_add(1);
            if i >= array.len() {
                return Err(GlyphWidthsMapError::IncompleteCIDEntry { cid });
            }
            match &array[i] {
                ObjectVariant::Array(widths_arr) => {
                    map.insert_explicit(cid, widths_arr)?;
                    i = i.saturating_add(1);
                }
                ObjectVariant::Integer(_) | ObjectVariant::Real(_) => {
                    // Need c_last and width
                    let c_last = array[i].as_number::<i64>()?;
                    let width_idx = i.saturating_add(1);
                    if width_idx >= array.len() {
                        return Err(GlyphWidthsMapError::MissingWidthForCIDRange { c_first: cid });
                    }
                    let width = array[width_idx].as_number::<f32>()?;
                    map.insert_uniform(cid, c_last, width)?;
                    i = i.saturating_add(2); // consumed c_last + width
                }
                other => {
                    return Err(GlyphWidthsMapError::UnexpectedElementAfterCID {
                        cid,
                        found_type: other.name(),
                    });
                }
            }
        }
        Ok(map)
    }

    /// Insert an explicit widths run starting at `cid` with the provided object array.
    fn insert_explicit(
        &mut self,
        cid: i64,
        widths_arr: &[ObjectVariant],
    ) -> Result<(), GlyphWidthsMapError> {
        if widths_arr.is_empty() {
            return Err(GlyphWidthsMapError::EmptyWidthsArray { cid });
        }
        if self.runs.contains_key(&cid) {
            return Err(GlyphWidthsMapError::DuplicateCIDStart { cid });
        }
        let widths = widths_arr
            .iter()
            .map(ObjectVariant::as_number::<f32>)
            .collect::<Result<Vec<_>, _>>()?;
        let len_i64 = i64::try_from(widths.len()).unwrap_or(i64::MAX);
        self.check_overlap(cid, len_i64)?;
        self.runs.insert(cid, WidthRun::Explicit(widths));
        Ok(())
    }

    /// Insert a uniform run [cid ..= c_last] with constant `width`.
    fn insert_uniform(
        &mut self,
        cid: i64,
        c_last: i64,
        width: f32,
    ) -> Result<(), GlyphWidthsMapError> {
        if c_last < cid {
            return Err(GlyphWidthsMapError::InvalidCIDRange {
                c_first: cid,
                c_last,
            });
        }
        // Check for duplicate start.
        if self.runs.contains_key(&cid) {
            return Err(GlyphWidthsMapError::DuplicateCIDStart { cid });
        }

        // Convert range boundaries to u64 for safe arithmetic.
        let c_last_u = u64::try_from(c_last).map_err(|_| GlyphWidthsMapError::InvalidCIDRange {
            c_first: cid,
            c_last,
        })?;
        let cid_u = u64::try_from(cid).map_err(|_| GlyphWidthsMapError::InvalidCIDRange {
            c_first: cid,
            c_last,
        })?;

        // Calculate the number of CIDs in the range [c_first, c_last] inclusively.
        // `checked_sub` and `checked_add` prevent overflow on large ranges.
        // The length is `c_last - c_first + 1`.
        let length_u64 = c_last_u
            .checked_sub(cid_u)
            .and_then(|d| d.checked_add(1))
            .ok_or(GlyphWidthsMapError::InvalidCIDRange {
                c_first: cid,
                c_last,
            })?;
        const MAX_RANGE: u64 = 1 << 20; // 1,048,576 cap
        if length_u64 > MAX_RANGE {
            return Err(GlyphWidthsMapError::RangeTooLarge {
                c_first: cid,
                c_last,
                length: length_u64,
            });
        }
        let len_u32 =
            u32::try_from(length_u64).map_err(|_| GlyphWidthsMapError::RangeTooLarge {
                c_first: cid,
                c_last,
                length: length_u64,
            })?;
        self.check_overlap(cid, i64::from(len_u32))?;
        self.runs.insert(
            cid,
            WidthRun::Uniform {
                width,
                len: len_u32,
            },
        );
        Ok(())
    }

    /// Overlap checker formerly implemented as an inline closure inside `from_array`.
    /// Ensures new run [start, start+len_i64-1] does not intersect any existing run.
    fn check_overlap(&self, start: i64, len_i64: i64) -> Result<(), GlyphWidthsMapError> {
        if len_i64 <= 0 {
            return Ok(());
        }
        let len_u64 = u64::try_from(len_i64).map_err(|_| GlyphWidthsMapError::RangeTooLarge {
            c_first: start,
            c_last: i64::MAX,
            length: u64::MAX,
        })?;
        let start_u = u64::try_from(start).map_err(|_| GlyphWidthsMapError::RangeTooLarge {
            c_first: start,
            c_last: i64::MAX,
            length: len_u64,
        })?;
        let end_u = start_u.checked_add(len_u64.saturating_sub(1)).ok_or(
            GlyphWidthsMapError::RangeTooLarge {
                c_first: start,
                c_last: i64::MAX,
                length: len_u64,
            },
        )?;
        let end: i64 = i64::try_from(end_u).map_err(|_| GlyphWidthsMapError::RangeTooLarge {
            c_first: start,
            c_last: i64::MAX,
            length: len_u64,
        })?;
        if let Some((&prev_start, prev_run)) = self.runs.range(..start).next_back() {
            let prev_len: i64 = match prev_run {
                WidthRun::Explicit(v) => i64::try_from(v.len()).unwrap_or(i64::MAX),
                WidthRun::Uniform { len, .. } => i64::from(*len),
            };
            let prev_end = prev_start.saturating_add(prev_len.saturating_sub(1));
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
    pub fn get_width(&self, character_id: i64) -> Option<f32> {
        let (start, run) = self.runs.range(..=character_id).next_back()?;
        let offset = character_id.saturating_sub(*start);
        if offset < 0 {
            return None;
        }
        match run {
            WidthRun::Explicit(widths) => widths.get(usize::try_from(offset).ok()?).copied(),
            WidthRun::Uniform { width, len } => {
                let off_u64 = u64::try_from(offset).ok()?;
                if off_u64 < u64::from(*len) {
                    Some(*width)
                } else {
                    None
                }
            }
        }
    }

    /// Returns the width for a CID, or the provided default if missing.
    pub fn get_width_or_default(&self, character_id: i64, default: f32) -> f32 {
        self.get_width(character_id).unwrap_or(default)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use pdf_object::ObjectVariant;

    // Helper to create a pdf_object::Value::Number for i64
    fn num_i64(n: i64) -> ObjectVariant {
        ObjectVariant::Integer(n)
    }

    // Helper to create a pdf_object::Value::Number for f32
    fn num_f32(n: f32) -> ObjectVariant {
        ObjectVariant::Real(f64::from(n))
    }

    // Helper to create a pdf_object::Value::Array
    fn arr(elements: Vec<ObjectVariant>) -> ObjectVariant {
        ObjectVariant::Array(elements)
    }

    #[test]
    fn test_from_array_empty() {
        let input_array = vec![];
        let glyph_widths_map = GlyphWidthsMap::from_array(&input_array).unwrap();
        assert!(glyph_widths_map.runs.is_empty());
    }

    #[test]
    fn test_from_array_single_entry() {
        // [ 0 [500 450] ]
        let input_array = vec![num_i64(0), arr(vec![num_f32(500.0), num_f32(450.0)])];
        let glyph_widths_map = GlyphWidthsMap::from_array(&input_array).unwrap();
        assert_eq!(glyph_widths_map.runs.len(), 1);
        assert_eq!(glyph_widths_map.get_width(0), Some(500.0));
        assert_eq!(glyph_widths_map.get_width(1), Some(450.0));
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
        let glyph_widths_map = GlyphWidthsMap::from_array(&input_array).unwrap();
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
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::IncompleteCIDEntry { cid: 0 })
        ));
    }

    #[test]
    fn test_from_array_widths_not_an_array() {
        // [ 0, 500 ] (500 is not an array)
        let input_array = vec![num_i64(0), num_f32(500.0)];
        let result = GlyphWidthsMap::from_array(&input_array);
        // Now parsed as start of range with missing width -> MissingWidthForCIDRange
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::MissingWidthForCIDRange { c_first: 0 })
        ));
    }

    #[test]
    fn test_get_width_empty_map() {
        let glyph_widths_map = GlyphWidthsMap {
            runs: BTreeMap::new(),
        };
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
    fn test_get_width_cid_negative_if_supported_by_map_keys() {
        // While PDF CIDs are typically non-negative, i64 allows it.
        let mut runs = BTreeMap::new();
        runs.insert(-5, WidthRun::Explicit(vec![200.0, 210.0]));
        runs.insert(0, WidthRun::Explicit(vec![300.0]));
        let glyph_widths_map = GlyphWidthsMap { runs };

        assert_eq!(glyph_widths_map.get_width(-5), Some(200.0));
        assert_eq!(glyph_widths_map.get_width(-4), Some(210.0));
        assert_eq!(glyph_widths_map.get_width(-6), None);
        assert_eq!(glyph_widths_map.get_width(-3), None);
        assert_eq!(glyph_widths_map.get_width(0), Some(300.0));
    }

    #[test]
    fn test_from_array_c_first_c_last_w_form_single_entry() {
        // [ 10 12 600 ] -> CIDs 10, 11, 12 have width 600
        let input_array = vec![num_i64(10), num_i64(12), num_f32(600.0)];
        let glyph_widths_map = GlyphWidthsMap::from_array(&input_array).unwrap();
        assert_eq!(glyph_widths_map.runs.len(), 1);
        assert_eq!(glyph_widths_map.get_width(10), Some(600.0));
        assert_eq!(glyph_widths_map.get_width(11), Some(600.0));
        assert_eq!(glyph_widths_map.get_width(12), Some(600.0));
    }

    #[test]
    fn test_from_array_c_first_c_last_w_form_c_first_equals_c_last() {
        // [ 5 5 300 ] -> CID 5 has width 300
        let input_array = vec![num_i64(5), num_i64(5), num_f32(300.0)];
        let glyph_widths_map = GlyphWidthsMap::from_array(&input_array).unwrap();
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
        let glyph_widths_map = GlyphWidthsMap::from_array(&input_array).unwrap();
        assert_eq!(glyph_widths_map.runs.len(), 3);
        assert_eq!(glyph_widths_map.get_width(0), Some(500.0));
        assert_eq!(glyph_widths_map.get_width(10), Some(600.0));
        assert_eq!(glyph_widths_map.get_width(11), Some(600.0));
        assert_eq!(glyph_widths_map.get_width(20), Some(700.0));
        assert_eq!(glyph_widths_map.get_width(21), Some(750.0));
    }

    #[test]
    fn test_from_array_error_empty_array() {
        // [ 0 [] ]
        let input_array = vec![num_i64(0), arr(vec![])];
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::EmptyWidthsArray { cid: 0 })
        ));
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
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::DuplicateCIDStart { cid: 0 })
        ));
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
        let result = GlyphWidthsMap::from_array(&input_array);
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
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::OverlappingRange { cid: 2 })
        ));
    }

    #[test]
    fn test_from_array_error_negative_cid() {
        // [ -1 [500] ]
        let input_array = vec![num_i64(-1), arr(vec![num_f32(500.0)])];
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::NegativeCID { cid: -1 })
        ));
    }

    #[test]
    fn test_from_array_error_range_too_large() {
        // choose range exceeding limit (MAX_RANGE = 1<<20) -> need > 1_048_576 length
        let start = 0i64;
        let end = (1i64 << 20) + 5; // length = 1_048_582
        let input_array = vec![num_i64(start), num_i64(end), num_f32(600.0)];
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(
            matches!(result, Err(GlyphWidthsMapError::RangeTooLarge { c_first: 0, c_last, .. }) if c_last == end)
        );
    }

    #[test]
    fn test_uniform_range_lookup() {
        // [ 10 12 600 ]
        let input_array = vec![num_i64(10), num_i64(12), num_f32(600.0)];
        let glyph_widths_map = GlyphWidthsMap::from_array(&input_array).unwrap();
        for cid in 10..=12 {
            assert_eq!(glyph_widths_map.get_width(cid), Some(600.0));
        }
        assert_eq!(glyph_widths_map.get_width(13), None);
    }

    #[test]
    fn test_from_array_error_c_last_less_than_c_first() {
        // [ 10 8 600 ]
        let input_array = vec![num_i64(10), num_i64(8), num_f32(600.0)];
        let result = GlyphWidthsMap::from_array(&input_array);
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
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::MissingWidthForCIDRange { c_first: 10 })
        ));
    }

    #[test]
    fn test_from_array_error_c_last_not_a_number() {
        // [ 10 "not_c_last" 600 ]
        let input_array = vec![
            num_i64(10),
            ObjectVariant::LiteralString("not_c_last".to_string()),
            num_f32(600.0),
        ];
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::UnexpectedElementAfterCID {
                cid: 10,
                found_type: "LiteralString"
            })
        ));
    }
}
