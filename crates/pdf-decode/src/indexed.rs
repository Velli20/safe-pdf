//! Palette expansion helpers for indexed PDF color spaces.

use crate::error::DecodeError;

/// Expands indexed palette values into their base component bytes.
pub fn expand_indexed_values(
    indices: &[u8],
    lookup: &[u8],
    hival: u8,
    base_components: usize,
) -> Result<Vec<u8>, DecodeError> {
    if base_components == 0 {
        return Err(DecodeError::InvalidComponentCount);
    }

    let mut out = Vec::with_capacity(indices.len().saturating_mul(base_components));
    for (pixel_index, &index) in indices.iter().enumerate() {
        let clamped_index = index.min(hival);
        let start = usize::from(clamped_index).saturating_mul(base_components);
        let end = start.saturating_add(base_components);
        let entry = lookup
            .get(start..end)
            .ok_or(DecodeError::PaletteLookupOutOfBounds {
                index: clamped_index,
                pixel_index,
                lookup_len: lookup.len(),
            })?;
        out.extend_from_slice(entry);
    }

    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn expand_indexed_values_supports_clamping() {
        let out = expand_indexed_values(&[2], &[10, 11, 12, 20, 21, 22], 1, 3).unwrap();

        assert_eq!(out, vec![20, 21, 22]);
    }

    #[test]
    fn expand_indexed_values_rejects_zero_components() {
        let err = expand_indexed_values(&[0], &[10], 0, 0)
            .expect_err("zero component palettes should fail");

        assert!(matches!(err, DecodeError::InvalidComponentCount));
    }

    #[test]
    fn expand_indexed_values_rejects_short_lookup() {
        let err = expand_indexed_values(&[1], &[10, 11, 12], 1, 3)
            .expect_err("lookup is too short for palette index 1");

        assert!(matches!(
            err,
            DecodeError::PaletteLookupOutOfBounds {
                index: 1,
                pixel_index: 0,
                lookup_len: 3
            }
        ));
    }
}
