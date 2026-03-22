use std::collections::HashMap;

use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::font::FontError;

pub struct SimpleFontGlyphWidthsMap;

impl SimpleFontGlyphWidthsMap {
    const KEY: &'static str = "Widths";

    /// Reads the `/Widths` array from a simple-font dictionary, mapping each
    /// character code in `FirstChar..=LastChar` to its glyph width.
    ///
    /// Per PDF spec, the array must contain exactly
    /// `(LastChar − FirstChar + 1)` entries. If the actual array is shorter we
    /// map only the entries present; if longer we ignore the surplus.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<HashMap<u16, f32>>, FontError> {
        // Check /Widths first — Standard 14 fonts may lack both /Widths and
        // /FirstChar, so we must not error on a missing /FirstChar when there
        // are no widths to parse.
        let Some(widths_obj) = dictionary.get(Self::KEY) else {
            return Ok(None);
        };

        let first_char = dictionary
            .get_or_err("FirstChar")?
            .try_number::<u16>(objects)?;

        let last_char = dictionary
            .get_or_err("LastChar")?
            .try_number::<u16>(objects)?;

        // Validate: FirstChar must not exceed LastChar.
        if first_char > last_char {
            // Treat an invalid range as if no widths were specified, rather than
            // returning an empty map that could be misinterpreted as "has widths".
            return Ok(None);
        }

        let arr = widths_obj.try_array(objects)?;

        // Per spec the array should have (LastChar - FirstChar + 1) entries.
        // Be lenient: use the minimum of the actual length and the expected
        // count so we never read past the array or map codes beyond LastChar.
        let range = last_char.saturating_sub(first_char);
        let expected_count = usize::from(range).saturating_add(1);
        let count = arr.len().min(expected_count);

        let mut widths = HashMap::new();
        for (i, w) in arr.iter().enumerate().take(count) {
            let Some(i_u16) = u16::try_from(i).ok() else {
                break;
            };
            let code = first_char.saturating_add(i_u16);
            let width = w.try_number::<f32>(objects)?;
            widths.insert(code, width);
        }

        Ok(Some(widths))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use pdf_object::{object_resolver::PassthroughResolver, object_variant::ObjectVariant};
    use std::collections::BTreeMap;

    fn make_dict(entries: Vec<(&str, ObjectVariant)>) -> Dictionary {
        let map: BTreeMap<String, ObjectVariant> = entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
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
        ObjectVariant::Array(items)
    }

    #[test]
    fn normal_widths_mapping() {
        let dict = make_dict(vec![
            ("FirstChar", int(32)),
            ("LastChar", int(34)),
            ("Widths", arr(vec![real(250.0), real(300.0), real(400.0)])),
        ]);
        let result = SimpleFontGlyphWidthsMap::from_dictionary(&dict, &PassthroughResolver)
            .unwrap()
            .unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[&32], 250.0);
        assert_eq!(result[&33], 300.0);
        assert_eq!(result[&34], 400.0);
    }

    #[test]
    fn missing_widths_returns_none() {
        let dict = make_dict(vec![("FirstChar", int(0)), ("LastChar", int(0))]);
        let result =
            SimpleFontGlyphWidthsMap::from_dictionary(&dict, &PassthroughResolver).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn no_widths_no_firstchar_returns_none() {
        // Standard 14 fonts may have neither /Widths nor /FirstChar.
        let dict = make_dict(vec![]);
        let result =
            SimpleFontGlyphWidthsMap::from_dictionary(&dict, &PassthroughResolver).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn first_char_greater_than_last_char_returns_empty() {
        let dict = make_dict(vec![
            ("FirstChar", int(100)),
            ("LastChar", int(50)),
            ("Widths", arr(vec![real(500.0)])),
        ]);
        let result =
            SimpleFontGlyphWidthsMap::from_dictionary(&dict, &PassthroughResolver).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn oversized_array_is_truncated() {
        // LastChar - FirstChar + 1 = 2, but array has 5 entries.
        let dict = make_dict(vec![
            ("FirstChar", int(10)),
            ("LastChar", int(11)),
            (
                "Widths",
                arr(vec![
                    real(100.0),
                    real(200.0),
                    real(300.0),
                    real(400.0),
                    real(500.0),
                ]),
            ),
        ]);
        let result = SimpleFontGlyphWidthsMap::from_dictionary(&dict, &PassthroughResolver)
            .unwrap()
            .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[&10], 100.0);
        assert_eq!(result[&11], 200.0);
        assert!(!result.contains_key(&12));
    }

    #[test]
    fn undersized_array_maps_available_entries() {
        // LastChar - FirstChar + 1 = 3, but array only has 1 entry.
        let dict = make_dict(vec![
            ("FirstChar", int(65)),
            ("LastChar", int(67)),
            ("Widths", arr(vec![real(600.0)])),
        ]);
        let result = SimpleFontGlyphWidthsMap::from_dictionary(&dict, &PassthroughResolver)
            .unwrap()
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[&65], 600.0);
    }

    #[test]
    fn single_char_range() {
        let dict = make_dict(vec![
            ("FirstChar", int(0)),
            ("LastChar", int(0)),
            ("Widths", arr(vec![real(1000.0)])),
        ]);
        let result = SimpleFontGlyphWidthsMap::from_dictionary(&dict, &PassthroughResolver)
            .unwrap()
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[&0], 1000.0);
    }
}
