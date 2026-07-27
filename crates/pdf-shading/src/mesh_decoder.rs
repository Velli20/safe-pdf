//! Decoding of coordinates and colors from packed mesh samples.

use num_traits::ToPrimitive;
use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::Function;
use pdf_graphics::{color::Color, point::Point};
use pdf_utils::BitReader;

use crate::error::PdfShadingError;

const VALID_COORDINATE_WIDTHS: [usize; 8] = [1, 2, 4, 8, 12, 16, 24, 32];
const VALID_COMPONENT_WIDTHS: [usize; 6] = [1, 2, 4, 8, 12, 16];
const VALID_FLAG_WIDTHS: [usize; 3] = [2, 4, 8];

/// Validated bit widths from a Type 4, 6, or 7 mesh dictionary.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MeshBitWidths {
    coordinate: usize,
    component: usize,
    flag: usize,
}

impl MeshBitWidths {
    /// Validates the three PDF mesh bit-width entries.
    pub(crate) fn new(
        coordinate: usize,
        component: usize,
        flag: usize,
    ) -> Result<Self, PdfShadingError> {
        validate_allowed_width(
            coordinate,
            &VALID_COORDINATE_WIDTHS,
            "BitsPerCoordinate must be 1, 2, 4, 8, 12, 16, 24, or 32",
        )?;
        validate_allowed_width(
            component,
            &VALID_COMPONENT_WIDTHS,
            "BitsPerComponent must be 1, 2, 4, 8, 12, or 16",
        )?;
        validate_allowed_width(flag, &VALID_FLAG_WIDTHS, "BitsPerFlag must be 2, 4, or 8")?;

        Ok(Self {
            coordinate,
            component,
            flag,
        })
    }

    /// Returns the width of each edge-flag field.
    pub(crate) fn flag(self) -> usize {
        self.flag
    }
}

/// Decodes mesh coordinates and color inputs according to a `/Decode` array.
///
/// A decoder borrows the parsed dictionary data while a mesh parser owns the
/// stream cursor. Keeping these responsibilities separate makes record
/// reconstruction independent from bit and color decoding.
pub(crate) struct MeshDecoder<'a> {
    widths: MeshBitWidths,
    decode: &'a [f32],
    color_input_count: usize,
    functions: &'a [Function],
    color_space: &'a ColorSpace,
}

impl<'a> MeshDecoder<'a> {
    /// Creates a decoder after validating `/Decode` against the color inputs.
    pub(crate) fn new(
        widths: MeshBitWidths,
        decode: &'a [f32],
        functions: &'a [Function],
        color_space: &'a ColorSpace,
        mesh_name: &str,
    ) -> Result<Self, PdfShadingError> {
        let color_input_count = if functions.is_empty() {
            color_space.num_color_components()
        } else {
            1
        };
        if color_input_count == 0 {
            return Err(invalid_mesh_data(format!(
                "{mesh_name} color space requires at least one component"
            )));
        }

        let expected_values = color_input_count.saturating_add(2).saturating_mul(2);
        if decode.len() != expected_values {
            return Err(invalid_mesh_data(format!(
                "{mesh_name} Decode array must contain {expected_values} values"
            )));
        }

        Ok(Self {
            widths,
            decode,
            color_input_count,
            functions,
            color_space,
        })
    }

    /// Reads and decodes one `(x, y)` coordinate pair.
    pub(crate) fn read_point(&self, reader: &mut BitReader<'_>) -> Result<Point, PdfShadingError> {
        let (x_min, x_max) = decode_pair(self.decode, 0, "X")?;
        let (y_min, y_max) = decode_pair(self.decode, 1, "Y")?;
        let x = self.read_sample(reader, self.widths.coordinate, x_min, x_max)?;
        let y = self.read_sample(reader, self.widths.coordinate, y_min, y_max)?;
        Ok(Point::new(x, y))
    }

    /// Reads mesh color inputs, applies optional functions, and converts them
    /// into the shading color space.
    pub(crate) fn read_color(&self, reader: &mut BitReader<'_>) -> Result<Color, PdfShadingError> {
        let inputs = (0..self.color_input_count)
            .map(|component| {
                let (min, max) =
                    decode_pair(self.decode, component.saturating_add(2), "component")?;
                self.read_sample(reader, self.widths.component, min, max)
            })
            .collect::<Result<Vec<_>, PdfShadingError>>()?;

        let components = self.apply_functions(&inputs)?;
        Ok(self.color_space.apply(&components)?)
    }

    fn read_sample(
        &self,
        reader: &mut BitReader<'_>,
        width: usize,
        min: f32,
        max: f32,
    ) -> Result<f32, PdfShadingError> {
        decode_sample(
            read_required_mesh_bits(reader, width)?.into(),
            width,
            min,
            max,
        )
    }

    fn apply_functions(&self, inputs: &[f32]) -> Result<Vec<f32>, PdfShadingError> {
        match self.functions {
            [] => Ok(inputs.to_vec()),
            [function] => Ok(function.apply(inputs)?),
            functions => functions
                .iter()
                .map(|function| {
                    function.apply(inputs)?.first().copied().ok_or_else(|| {
                        invalid_mesh_data(
                            "Mesh shading function did not return a color component".to_string(),
                        )
                    })
                })
                .collect(),
        }
    }
}

/// Reads an optional mesh field while distinguishing clean EOF from truncation.
pub(crate) fn read_mesh_bits(
    reader: &mut BitReader<'_>,
    width: usize,
) -> Result<Option<u32>, PdfShadingError> {
    if !(1..=32).contains(&width) {
        return Err(invalid_mesh_data(
            "Mesh bit-field widths must be in 1..=32".to_string(),
        ));
    }
    let width = u8::try_from(width)
        .map_err(|_| invalid_mesh_data("Mesh sample width is too large".to_string()))?;
    if reader.exhausted() {
        return Ok(None);
    }

    reader
        .read_bits_u32(width)
        .map(Some)
        .ok_or_else(|| invalid_mesh_data("Mesh stream ended in the middle of a sample".to_string()))
}

/// Reads a mesh field that is required to complete the current record.
fn read_required_mesh_bits(
    reader: &mut BitReader<'_>,
    width: usize,
) -> Result<u32, PdfShadingError> {
    read_mesh_bits(reader, width)?
        .ok_or_else(|| invalid_mesh_data("Mesh stream ended unexpectedly".to_string()))
}

fn validate_allowed_width(
    width: usize,
    allowed: &[usize],
    reason: &str,
) -> Result<(), PdfShadingError> {
    if allowed.contains(&width) {
        Ok(())
    } else {
        Err(invalid_mesh_data(reason.to_string()))
    }
}

fn decode_pair(
    decode: &[f32],
    pair_index: usize,
    label: &str,
) -> Result<(f32, f32), PdfShadingError> {
    let first_index = pair_index.saturating_mul(2);
    let second_index = first_index.saturating_add(1);
    let min = decode
        .get(first_index)
        .copied()
        .ok_or_else(|| invalid_mesh_data(format!("Decode array is missing {label} minimum")))?;
    let max = decode
        .get(second_index)
        .copied()
        .ok_or_else(|| invalid_mesh_data(format!("Decode array is missing {label} maximum")))?;
    Ok((min, max))
}

fn decode_sample(code: u64, width: usize, min: f32, max: f32) -> Result<f32, PdfShadingError> {
    let shift = u32::try_from(width)
        .map_err(|_| invalid_mesh_data("Mesh sample width is too large".to_string()))?;
    let code_max = 1_u64
        .checked_shl(shift)
        .ok_or_else(|| invalid_mesh_data("Mesh sample width overflowed".to_string()))?
        .saturating_sub(1);
    let code = code
        .to_f32()
        .ok_or_else(|| invalid_mesh_data("Mesh sample is not representable as f32".to_string()))?;
    let code_max = code_max.to_f32().ok_or_else(|| {
        invalid_mesh_data("Mesh sample range is not representable as f32".to_string())
    })?;

    Ok(min + (code / code_max) * (max - min))
}

fn invalid_mesh_data(reason: impl Into<String>) -> PdfShadingError {
    PdfShadingError::InvalidShadingMeshData {
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "../tests/mesh_decoder.rs"]
mod tests;
