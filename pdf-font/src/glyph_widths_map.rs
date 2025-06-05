use std::collections::HashMap;

use pdf_object::array::Array;

use crate::error::FontError;

/// A map storing glyph widths for specific Character IDs (CIDs).
///
/// This map is populated from the `/W` array in the CIDFont dictionary.
/// According to the PDF 1.7 specification (Section 9.7.4.3, "Glyph Metrics in CIDFonts"),
/// the `/W` array defines widths for CIDs. This implementation parses entries of two forms:
/// 1. `c [w1 w2 ... wn]`: `c` is the starting CID, and `w1, ..., wn` are the widths
///    for `n` consecutive CIDs starting from `c`.
/// 2. `c_first c_last w`: `c_first` and `c_last` define a range of CIDs (inclusive),
///    and `w` is the common width for all CIDs in that range.
/// The key in the `HashMap` is the starting `character_id` (CID), and the `Vec<f32>`
/// contains the widths for that CID and subsequent CIDs.
pub struct GlyphWidthsMap {
    map: HashMap<i64, Vec<f32>>,
}

impl GlyphWidthsMap {
    pub fn from_array(array: &Array) -> Result<GlyphWidthsMap, FontError> {
        // The "W" array (Widths) defines widths for specific CIDs.
        // PDF 1.7 spec, section 9.7.4.3 "Glyph Metrics in CIDFonts".
        // This parser handles two forms for entries in the /W array:
        // 1. c [w1 w2 ... wn]
        //    where 'c' is an integer (starting CID), and [w1 w2 ... wn] is a PDF array
        //    of numbers representing widths for CIDs c, c+1, ..., c+n-1.
        // 2. c_first c_last w
        //    where 'c_first' and 'c_last' are integers defining a range of CIDs,
        //    and 'w' is a number representing the width for all CIDs from
        //    c_first to c_last, inclusive.
        //
        // Example: [ 0 [500 450], 10 [600], 20 22 750 ]
        // - CID 0 has width 500, CID 1 has width 450.
        // - CID 10 has width 600.
        // - CIDs 20, 21, 22 all have width 750.
        //
        // The array is processed sequentially; a later entry can override an earlier one
        // for a particular CID (though this implementation stores ranges, so overlaps
        // are based on the starting CID of a range).

        let mut widths_map = HashMap::new();
        let mut iter = array.0.iter();

        while let Some(cid_or_c_first_element) = iter.next() {
            let c_or_c_first = cid_or_c_first_element.as_number::<i64>()?;

            // The next element determines the format.
            if let Some(next_element) = iter.next() {
                if let Some(widths_pdf_array) = next_element.as_array() {
                    // Form: c [w1 w2 ... wn]
                    // c_or_c_first is 'c'
                    let mut parsed_widths = Vec::new();
                    for width_value in &widths_pdf_array.0 {
                        let width_f32 = width_value.as_number::<f32>()?;
                        parsed_widths.push(width_f32);
                    }
                    widths_map.insert(c_or_c_first, parsed_widths);
                } else if let Ok(c_last) = next_element.as_number::<i64>() {
                    // Form: c_first c_last w
                    // c_or_c_first is 'c_first', next_element (parsed as c_last) is 'c_last'
                    if c_last < c_or_c_first {
                        return Err(FontError::InvalidFontDescriptor(
                            "Invalid /W array (form c_first c_last w): c_last cannot be less than c_first.",
                        ));
                    }

                    if let Some(width_element) = iter.next() {
                        let width = width_element.as_number::<f32>()?;
                        let num_cids = (c_last - c_or_c_first + 1) as usize;
                        let parsed_widths: Vec<f32> =
                            std::iter::repeat(width).take(num_cids).collect();
                        widths_map.insert(c_or_c_first, parsed_widths);
                    } else {
                        return Err(FontError::InvalidFontDescriptor(
                            "Invalid /W array (form c_first c_last w): missing width 'w' after c_last.",
                        ));
                    }
                } else {
                    // The element after c_or_c_first is neither an array (for form 1)
                    // nor an integer (for c_last in form 2).
                    return Err(FontError::InvalidFontDescriptor(
                        "Invalid /W array: expected an array of widths or a c_last integer after a CID.",
                    ));
                }
            } else {
                // c_or_c_first was found without any subsequent elements.
                return Err(FontError::InvalidFontDescriptor(
                    "Invalid /W array: CID found without a corresponding width array or c_last value.",
                ));
            }
        }

        Ok(Self { map: widths_map })
    }

    pub fn get_width(&self, character_id: i64) -> Option<f32> {
        // Iterate through all stored CID ranges in the map.
        // Each entry consists of a starting CID and a vector of widths for consecutive CIDs.
        for (start_cid_key, widths_for_range) in &self.map {
            let start_cid = *start_cid_key; // Dereference the key to get the i64 value.

            // Check if the requested character_id is potentially within the range
            // covered by this entry (i.e., character_id is not before start_cid).
            if character_id >= start_cid {
                // Calculate the offset of the character_id from the start_cid of this range.
                let offset = character_id - start_cid; // This will be an i64 value.

                // The offset must be non-negative (which is guaranteed by `character_id >= start_cid`)
                // and must be less than the number of widths stored for this range.
                // We cast `widths_for_range.len()` (usize) to i64 for a safe comparison.
                // This assumes the length of any width vector fits within an i64, which is practical.
                if offset < (widths_for_range.len() as i64) {
                    // If the offset is valid, it means the character_id falls within this range.
                    // Cast the offset to usize to use it as an index.
                    let index = offset as usize;

                    // Retrieve the width. Since f32 is Copy, we can directly access and return.
                    // The bounds check `offset < (widths_for_range.len() as i64)` ensures
                    // that `index` will be a valid index for `widths_for_range`.
                    return Some(widths_for_range[index]);
                }
                // If offset is too large, character_id is past the end of this specific range.
                // Continue to the next entry in the map.
            }
            // If character_id < start_cid, this range starts after the character_id,
            // so we continue to the next entry.
        }

        // If the loop completes, it means the character_id was not found in any defined range.
        None
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
        assert!(matches!(result, Err(FontError::ObjectError(_))));
    }

    #[test]
    fn test_from_array_missing_widths_array() {
        // [ 0 ] (missing widths array)
        let input_array = Array(vec![num_i64(0)]);
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(FontError::InvalidFontDescriptor(
                "Invalid /W array: CID found without a corresponding width array."
            ))
        ));
    }

    #[test]
    fn test_from_array_widths_not_an_array() {
        // [ 0, 500 ] (500 is not an array)
        let input_array = Array(vec![num_i64(0), num_f32(500.0)]);
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(FontError::InvalidFontDescriptor(
                "Invalid /W array: expected an array of widths following a CID."
            ))
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
            Err(FontError::InvalidFontDescriptor(
                "Invalid /W array (form c_first c_last w): c_last cannot be less than c_first."
            ))
        ));
    }

    #[test]
    fn test_from_array_error_missing_w_in_c_first_c_last_w() {
        // [ 10 12 ] (missing w)
        let input_array = Array(vec![num_i64(10), num_i64(12)]);
        let result = GlyphWidthsMap::from_array(&input_array);
        assert!(matches!(
            result,
            Err(FontError::InvalidFontDescriptor(
                "Invalid /W array (form c_first c_last w): missing width 'w' after c_last."
            ))
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
        assert!(matches!(result, Err(FontError::ObjectError(_))));
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
            Err(FontError::InvalidFontDescriptor(
                "Invalid /W array: expected an array of widths or a c_last integer after a CID."
            ))
        ));
    }
}
