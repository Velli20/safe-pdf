use pdf_object::{Value, array::Array, error::ObjectError};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during GlyphWidthsMap parsing from a /W array.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum GlyphWidthsMapError {
    /// Error converting a PDF value to a number.
    #[error("Failed to convert PDF value to number for '{entry_description}': {source}")]
    NumericConversionError {
        entry_description: &'static str,
        #[source]
        source: ObjectError,
    },
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
}

/// Stores glyph widths for CIDs, parsed from the /W array.
///
/// The internal map uses the starting CID as the key and a vector of
/// widths as the value. For a range, the vector contains repeated
/// widths; for an explicit array, the vector contains the widths as specified.
pub struct GlyphWidthsMap {
    map: HashMap<i64, Vec<f32>>,
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
    pub fn from_array(array: &Array) -> Result<Self, GlyphWidthsMapError> {
        let mut map = HashMap::new();
        let mut iter = array.0.iter();

        while let Some(cid_val) = iter.next() {
            // Parse the starting CID (or c_first)
            let cid = cid_val.as_number::<i64>().map_err(|e| {
                GlyphWidthsMapError::NumericConversionError {
                    entry_description: "CID or c_first",
                    source: e,
                }
            })?;

            // Next element must be either an array of widths or c_last
            let Some(next) = iter.next() else {
                return Err(GlyphWidthsMapError::IncompleteCIDEntry { cid });
            };

            match next {
                // Case: [c_first [w1 ... wn]]
                Value::Array(widths_arr) => {
                    let widths = widths_arr
                        .0
                        .iter()
                        .map(|w| {
                            w.as_number::<f32>().map_err(|e| {
                                GlyphWidthsMapError::NumericConversionError {
                                    entry_description: "width in [w1...wn] array",
                                    source: e,
                                }
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    map.insert(cid, widths);
                }
                // Case: [c_first c_last w]
                Value::Number(_) => {
                    let c_last = next.as_number::<i64>().map_err(|e| {
                        GlyphWidthsMapError::NumericConversionError {
                            entry_description: "c_last",
                            source: e,
                        }
                    })?;
                    if c_last < cid {
                        return Err(GlyphWidthsMapError::InvalidCIDRange {
                            c_first: cid,
                            c_last,
                        });
                    }
                    let Some(width_val) = iter.next() else {
                        return Err(GlyphWidthsMapError::MissingWidthForCIDRange { c_first: cid });
                    };
                    let width = width_val.as_number::<f32>().map_err(|e| {
                        GlyphWidthsMapError::NumericConversionError {
                            entry_description: "width 'w' for c_first c_last w form",
                            source: e,
                        }
                    })?;
                    let count = (c_last - cid + 1) as usize;
                    map.insert(cid, vec![width; count]);
                }
                // Unexpected element type
                other => {
                    return Err(GlyphWidthsMapError::UnexpectedElementAfterCID {
                        cid,
                        found_type: other.name(),
                    });
                }
            }
        }
        Ok(Self { map })
    }

    /// Returns the width for a given CID (character ID), if present.
    ///
    /// # Arguments
    /// * `character_id` - The CID to look up.
    ///
    /// # Returns
    /// * `Some(width)` if the width is found, or `None` if not present.
    pub fn get_width(&self, character_id: i64) -> Option<f32> {
        self.map.iter().find_map(|(&start, widths)| {
            let offset = character_id - start;
            if offset >= 0 && (offset as usize) < widths.len() {
                Some(widths[offset as usize])
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_object::{Value, literal_string::LiteralString, number::Number};

    // Helper to create a pdf_object::Value::Number for i64
    fn num_i64(n: i64) -> Value {
        Value::Number(Number::new(n))
    }

    // Helper to create a pdf_object::Value::Number for f32
    fn num_f32(n: f32) -> Value {
        Value::Number(Number::new(n as f64))
    }

    // Helper to create a pdf_object::Value::Array
    fn arr(elements: Vec<Value>) -> Value {
        Value::Array(Array::new(elements))
    }

    #[test]
    fn test_from_array_empty() {
        let input_array = Array(vec![]);
        let glyph_widths_map = GlyphWidthsMap::from_array(&input_array).unwrap();
        assert!(glyph_widths_map.map.is_empty());
    }

    #[test]
    fn test_from_array_single_entry() {
        // [ 0 [500 450] ]
        let input_array = Array(vec![num_i64(0), arr(vec![num_f32(500.0), num_f32(450.0)])]);
        let glyph_widths_map = GlyphWidthsMap::from_array(&input_array).unwrap();
        assert_eq!(glyph_widths_map.map.len(), 1);
        assert_eq!(glyph_widths_map.map.get(&0).unwrap(), &vec![500.0, 450.0]);
    }

    #[test]
    fn test_from_array_multiple_entries() {
        // [ 0 [500], 10 [600 650], 20 [700] ]
        let input_array = Array(vec![
            num_i64(0),
            arr(vec![num_f32(500.0)]),
            num_i64(10),
            arr(vec![num_f32(600.0), num_f32(650.0)]),
            num_i64(20),
            arr(vec![num_f32(700.0)]),
        ]);
        let glyph_widths_map = GlyphWidthsMap::from_array(&input_array).unwrap();
        assert_eq!(glyph_widths_map.map.len(), 3);
        assert_eq!(glyph_widths_map.map.get(&0).unwrap(), &vec![500.0]);
        assert_eq!(glyph_widths_map.map.get(&10).unwrap(), &vec![600.0, 650.0]);
        assert_eq!(glyph_widths_map.map.get(&20).unwrap(), &vec![700.0]);
    }

    #[test]
    fn test_from_array_invalid_cid_not_a_number() {
        let input_array = Array(vec![
            Value::LiteralString(LiteralString::new("not_a_cid".to_string())),
            arr(vec![num_f32(500.0)]),
        ]);
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::NumericConversionError { .. })
        ));
    }

    #[test]
    fn test_from_array_missing_widths_array() {
        // [ 0 ] (missing widths array)
        let input_array = Array(vec![num_i64(0)]);
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::IncompleteCIDEntry { cid: 0 })
        ));
    }

    #[test]
    fn test_from_array_widths_not_an_array() {
        // [ 0, 500 ] (500 is not an array)
        let input_array = Array(vec![num_i64(0), num_f32(500.0)]);
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::UnexpectedElementAfterCID {
                cid: 0,
                found_type: "Number"
            })
        ));
    }

    #[test]
    fn test_get_width_empty_map() {
        let glyph_widths_map = GlyphWidthsMap {
            map: HashMap::new(),
        };
        assert_eq!(glyph_widths_map.get_width(0), None);
    }

    #[test]
    fn test_get_width_exact_match_start_cid() {
        let mut map = HashMap::new();
        map.insert(10, vec![500.0, 550.0]);
        let glyph_widths_map = GlyphWidthsMap { map };
        assert_eq!(glyph_widths_map.get_width(10), Some(500.0));
    }

    #[test]
    fn test_get_width_within_range() {
        let mut map = HashMap::new();
        map.insert(10, vec![500.0, 550.0, 600.0]); // CIDs 10, 11, 12
        let glyph_widths_map = GlyphWidthsMap { map };
        assert_eq!(glyph_widths_map.get_width(11), Some(550.0));
    }

    #[test]
    fn test_get_width_end_of_range() {
        let mut map = HashMap::new();
        map.insert(10, vec![500.0, 550.0, 600.0]); // CIDs 10, 11, 12
        let glyph_widths_map = GlyphWidthsMap { map };
        assert_eq!(glyph_widths_map.get_width(12), Some(600.0));
    }

    #[test]
    fn test_get_width_cid_before_range() {
        let mut map = HashMap::new();
        map.insert(10, vec![500.0]);
        let glyph_widths_map = GlyphWidthsMap { map };
        assert_eq!(glyph_widths_map.get_width(9), None);
    }

    #[test]
    fn test_get_width_cid_after_range() {
        let mut map = HashMap::new();
        map.insert(10, vec![500.0, 550.0]); // CIDs 10, 11
        let glyph_widths_map = GlyphWidthsMap { map };
        assert_eq!(glyph_widths_map.get_width(12), None);
    }

    #[test]
    fn test_get_width_cid_between_ranges() {
        let mut map = HashMap::new();
        map.insert(0, vec![100.0, 110.0]); // CIDs 0, 1
        map.insert(10, vec![500.0, 550.0]); // CIDs 10, 11
        let glyph_widths_map = GlyphWidthsMap { map };
        assert_eq!(glyph_widths_map.get_width(5), None); // Between ranges
        assert_eq!(glyph_widths_map.get_width(0), Some(100.0));
        assert_eq!(glyph_widths_map.get_width(1), Some(110.0));
        assert_eq!(glyph_widths_map.get_width(10), Some(500.0));
        assert_eq!(glyph_widths_map.get_width(11), Some(550.0));
    }

    #[test]
    fn test_get_width_multiple_ranges_correct_selection() {
        let mut map = HashMap::new();
        map.insert(100, vec![1000.0]);
        map.insert(0, vec![100.0, 110.0, 120.0]);
        map.insert(50, vec![500.0, 510.0]);
        let glyph_widths_map = GlyphWidthsMap { map };

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
        let mut map = HashMap::new();
        map.insert(-5, vec![200.0, 210.0]); // CIDs -5, -4
        map.insert(0, vec![300.0]);
        let glyph_widths_map = GlyphWidthsMap { map };

        assert_eq!(glyph_widths_map.get_width(-5), Some(200.0));
        assert_eq!(glyph_widths_map.get_width(-4), Some(210.0));
        assert_eq!(glyph_widths_map.get_width(-6), None);
        assert_eq!(glyph_widths_map.get_width(-3), None);
        assert_eq!(glyph_widths_map.get_width(0), Some(300.0));
    }

    #[test]
    fn test_from_array_c_first_c_last_w_form_single_entry() {
        // [ 10 12 600 ] -> CIDs 10, 11, 12 have width 600
        let input_array = Array(vec![num_i64(10), num_i64(12), num_f32(600.0)]);
        let glyph_widths_map = GlyphWidthsMap::from_array(&input_array).unwrap();
        assert_eq!(glyph_widths_map.map.len(), 1);
        assert_eq!(
            glyph_widths_map.map.get(&10).unwrap(),
            &vec![600.0, 600.0, 600.0]
        );
    }

    #[test]
    fn test_from_array_c_first_c_last_w_form_c_first_equals_c_last() {
        // [ 5 5 300 ] -> CID 5 has width 300
        let input_array = Array(vec![num_i64(5), num_i64(5), num_f32(300.0)]);
        let glyph_widths_map = GlyphWidthsMap::from_array(&input_array).unwrap();
        assert_eq!(glyph_widths_map.map.len(), 1);
        assert_eq!(glyph_widths_map.map.get(&5).unwrap(), &vec![300.0]);
    }

    #[test]
    fn test_from_array_mixed_forms() {
        // [ 0 [500], 10 11 600, 20 [700 750] ]
        let input_array = Array(vec![
            num_i64(0),
            arr(vec![num_f32(500.0)]),
            num_i64(10),
            num_i64(11),
            num_f32(600.0),
            num_i64(20),
            arr(vec![num_f32(700.0), num_f32(750.0)]),
        ]);
        let glyph_widths_map = GlyphWidthsMap::from_array(&input_array).unwrap();
        assert_eq!(glyph_widths_map.map.len(), 3);
        assert_eq!(glyph_widths_map.map.get(&0).unwrap(), &vec![500.0]);
        assert_eq!(glyph_widths_map.map.get(&10).unwrap(), &vec![600.0, 600.0]);
        assert_eq!(glyph_widths_map.map.get(&20).unwrap(), &vec![700.0, 750.0]);
    }

    #[test]
    fn test_from_array_error_c_last_less_than_c_first() {
        // [ 10 8 600 ]
        let input_array = Array(vec![num_i64(10), num_i64(8), num_f32(600.0)]);
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
        let input_array = Array(vec![num_i64(10), num_i64(12)]);
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::MissingWidthForCIDRange { c_first: 10 })
        ));
    }

    #[test]
    fn test_from_array_error_w_not_a_number() {
        // [ 10 12 "not_a_width" ]
        let input_array = Array(vec![
            num_i64(10),
            num_i64(12),
            Value::LiteralString(LiteralString::new("not_a_width".to_string())),
        ]);
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(GlyphWidthsMapError::NumericConversionError { .. })
        ));
    }

    #[test]
    fn test_from_array_error_c_last_not_a_number() {
        // [ 10 "not_c_last" 600 ]
        let input_array = Array(vec![
            num_i64(10),
            Value::LiteralString(LiteralString::new("not_c_last".to_string())),
            num_f32(600.0),
        ]);
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
