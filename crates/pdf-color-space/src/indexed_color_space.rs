use std::sync::Arc;

use pdf_graphics::color::Color;
use pdf_object::{object_resolver::ObjectResolver, object_variant::ObjectVariant};

use crate::{
    color_space::ColorSpace, color_space_reader::parse_color_space_object, error::ColorSpaceError,
};

/// Indexed (palette-based) color space.
///
/// Maps integer indices to colors in a base color space via a lookup table.
/// Commonly used for images with a limited color palette.
#[derive(Debug, Clone)]
pub struct IndexedColorSpace {
    /// The underlying color space for palette entries.
    pub base: Box<ColorSpace>,
    /// Maximum valid index value (0 to 255). The palette contains `hival + 1` entries.
    pub hival: u8,
    /// Raw lookup table bytes. Each entry contains `base.num_color_components()` bytes.
    pub lookup: Arc<Vec<u8>>,
}

/// Parses an Indexed color space: `[/Indexed base hival lookup]`
///
/// - `base`: The base color space for palette entries
/// - `hival`: Maximum index value (0-255)
/// - `lookup`: Lookup table (string or stream)
pub(crate) fn parse_indexed_color_space(
    objects: &dyn ObjectResolver,
    arr: &[ObjectVariant],
    depth: usize,
) -> Result<ColorSpace, ColorSpaceError> {
    // Expected format: [/Indexed base hival lookup]
    let [_, base, hival, lookup] = arr else {
        return Err(ColorSpaceError::InvalidColorSpace {
            description: format!("/Indexed requires 4 elements, found {}", arr.len()),
        });
    };

    let base_cs = parse_color_space_object(objects, base, depth)?;
    let hival = hival.try_number::<u8>(objects)?;
    let lookup = extract_lookup_table(objects, lookup)?;

    Ok(ColorSpace::Indexed(IndexedColorSpace {
        base: Box::new(base_cs),
        hival,
        lookup,
    }))
}

/// Extracts the lookup table bytes from an Indexed color space.
///
/// The lookup table can be either a string/hex-string or a stream.
fn extract_lookup_table(
    objects: &dyn ObjectResolver,
    lookup: &ObjectVariant,
) -> Result<Arc<Vec<u8>>, ColorSpaceError> {
    if let Ok(data) = lookup.try_bytes(objects) {
        return Ok(Arc::new(data.to_vec()));
    }
    Ok(lookup.try_stream(objects)?.shared_data())
}

/// Converts a raw palette entry (byte slice) to a [`Color`] using the given base color space.
///
/// Each byte in `entry` encodes a color component in the range 0–255.
pub(crate) fn indexed_entry_to_color(
    base: &ColorSpace,
    entry: &[u8],
) -> Result<Color, ColorSpaceError> {
    match base {
        ColorSpace::DeviceGray => {
            let g = f32::from(*entry.first().ok_or_else(|| {
                ColorSpaceError::IndexedColorSpaceError(
                    "Gray Indexed palette entry is empty".into(),
                )
            })?) / 255.0;
            Ok(Color::from_gray(g))
        }
        ColorSpace::DeviceRGB => match *entry {
            [r, g, b] => Ok(Color::from_rgb(
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
            )),
            _ => Err(ColorSpaceError::IndexedColorSpaceError(format!(
                "Indexed RGB palette entry expected 3 bytes, got {}",
                entry.len()
            ))),
        },
        ColorSpace::DeviceCMYK => match *entry {
            [c, m, y, k] => Ok(Color::from_cmyk(
                f32::from(c) / 255.0,
                f32::from(m) / 255.0,
                f32::from(y) / 255.0,
                f32::from(k) / 255.0,
            )),
            _ => Err(ColorSpaceError::IndexedColorSpaceError(format!(
                "Indexed CMYK palette entry expected 4 bytes, got {}",
                entry.len()
            ))),
        },
        _ => {
            // Generic fallback: normalise each byte to [0.0, 1.0] and delegate
            // to the base color space.
            let n = base.num_color_components();
            if entry.len() != n {
                return Err(ColorSpaceError::IndexedColorSpaceError(format!(
                    "Indexed palette entry: expected {n} bytes, got {}",
                    entry.len()
                )));
            }
            let components: Vec<f32> = entry.iter().map(|&b| f32::from(b) / 255.0).collect();
            base.apply(&components)
        }
    }
}

impl IndexedColorSpace {
    pub(crate) fn apply(&self, components: &[f32]) -> Result<Color, ColorSpaceError> {
        let index = components
            .first()
            .copied()
            .ok_or(ColorSpaceError::InsufficientComponents(1, components.len()))?;
        let rounded = index.round();
        if !rounded.is_finite() {
            return Err(ColorSpaceError::Unsupported(format!(
                "Indexed color index out of range: {rounded}"
            )));
        }

        let bounded = rounded.clamp(0.0, f32::from(self.hival));
        let idx = (0..=self.hival)
            .find(|candidate| f32::from(*candidate) == bounded)
            .map(usize::from)
            .ok_or_else(|| {
                ColorSpaceError::Unsupported(format!("Indexed color index out of range: {rounded}"))
            })?;
        let n = self.base.num_color_components();
        let offset = idx.saturating_mul(n);
        let entry = self
            .lookup
            .get(offset..offset.saturating_add(n))
            .ok_or_else(|| {
                ColorSpaceError::Unsupported(format!(
                    "Indexed color lookup out of bounds at index {idx}"
                ))
            })?;
        indexed_entry_to_color(&self.base, entry)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver, stream::StreamObject,
    };

    use super::*;

    #[test]
    fn stream_lookup_shares_decoded_stream_bytes() {
        let stream = StreamObject::new(
            1,
            0,
            Box::new(Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new())),
            vec![1, 2, 3, 4],
        );
        let stream_data = stream.shared_data();
        let lookup = ObjectVariant::Stream(stream);

        let extracted = extract_lookup_table(&PassthroughResolver, &lookup)
            .expect("stream lookup table should parse");

        assert!(Arc::ptr_eq(&extracted, &stream_data));
    }

    #[test]
    fn string_lookups_preserve_their_bytes() {
        for lookup in [
            ObjectVariant::LiteralString(vec![1, 2, 3]),
            ObjectVariant::HexString(vec![4, 5, 6]),
        ] {
            let expected = lookup
                .try_bytes(&PassthroughResolver)
                .expect("test lookup should contain bytes")
                .to_vec();
            let extracted = extract_lookup_table(&PassthroughResolver, &lookup)
                .expect("string lookup table should parse");

            assert_eq!(extracted.as_slice(), expected.as_slice());
        }
    }

    fn indexed_rgb() -> IndexedColorSpace {
        IndexedColorSpace {
            base: Box::new(ColorSpace::DeviceRGB),
            hival: 7,
            lookup: Arc::new(vec![
                0, 128, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 255, 255, 255, 0, 255, 255, 255, 0,
                243, 128, 255,
            ]),
        }
    }

    #[test]
    fn apply_clamps_negative_indices_to_zero() {
        let color = indexed_rgb()
            .apply(&[-17.0])
            .expect("negative indexed color should clamp to zero");

        assert_eq!(color, Color::from_rgb(0.0, 128.0 / 255.0, 0.0));
    }

    #[test]
    fn apply_clamps_high_indices_to_hival() {
        let color = indexed_rgb()
            .apply(&[17.0])
            .expect("high indexed color should clamp to hival");

        assert_eq!(color, Color::from_rgb(243.0 / 255.0, 128.0 / 255.0, 1.0));
    }

    #[test]
    fn apply_rounds_before_clamping_to_hival() {
        let color = indexed_rgb()
            .apply(&[6.5])
            .expect("fractional indexed color should round before clamping");

        assert_eq!(color, Color::from_rgb(243.0 / 255.0, 128.0 / 255.0, 1.0));
    }

    #[test]
    fn apply_rejects_non_finite_indices() {
        let err = indexed_rgb()
            .apply(&[f32::NAN])
            .expect_err("nan indexed color should fail");

        assert!(matches!(err, ColorSpaceError::Unsupported(_)));

        let err = indexed_rgb()
            .apply(&[f32::INFINITY])
            .expect_err("infinite indexed color should fail");

        assert!(matches!(err, ColorSpaceError::Unsupported(_)));
    }
}
