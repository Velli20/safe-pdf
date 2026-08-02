//! Decoding of coordinates and colors from packed mesh samples.

use num_traits::ToPrimitive;
use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::Function;
use pdf_graphics::{color::Color, point::Point};
use pdf_utils::BitReader;
use thiserror::Error;

use crate::{error::PdfShadingError, mesh_bit_widths::MeshBitWidths};

/// Errors produced while validating and decoding packed mesh samples.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MeshDecoderError {
    /// `/BitsPerCoordinate` is not one of the widths permitted by the PDF specification.
    #[error("invalid /BitsPerCoordinate value {value}")]
    InvalidBitsPerCoordinate { value: usize },
    /// `/BitsPerComponent` is not one of the widths permitted by the PDF specification.
    #[error("invalid /BitsPerComponent value {value}")]
    InvalidBitsPerComponent { value: usize },
    /// `/BitsPerFlag` is not one of the widths permitted by the PDF specification.
    #[error("invalid /BitsPerFlag value {value}")]
    InvalidBitsPerFlag { value: usize },
    /// The selected color space does not define any color components.
    #[error("{mesh} color space requires at least one component")]
    MissingColorComponents { mesh: &'static str },
    /// `/Decode` does not contain exactly the required coordinate and component ranges.
    #[error("{mesh} Decode array contains {actual} values; expected {expected}")]
    InvalidDecodeLength {
        mesh: &'static str,
        expected: usize,
        actual: usize,
    },
    /// A shading function did not produce its required color component.
    #[error("mesh shading function did not return a color component")]
    MissingFunctionColorComponent,
    /// A requested packed field width cannot be read by the mesh decoder.
    #[error("invalid mesh bit-field width {width}; expected 1..=32")]
    InvalidBitFieldWidth { width: usize },
    /// The stream ended after only part of a packed sample was available.
    #[error("mesh stream ended in the middle of a sample")]
    TruncatedSample,
    /// The stream ended before a required sample began.
    #[error("mesh stream ended unexpectedly")]
    UnexpectedEndOfStream,
    /// A `/Decode` range does not include its minimum value.
    #[error("Decode array is missing {label} minimum")]
    MissingDecodeMinimum { label: &'static str },
    /// A `/Decode` range does not include its maximum value.
    #[error("Decode array is missing {label} maximum")]
    MissingDecodeMaximum { label: &'static str },
    /// A sample value cannot be represented by the decoder's floating-point type.
    #[error("mesh sample is not representable as f32")]
    SampleNotRepresentable,
    /// A sample's maximum encoded value cannot be represented by the decoder's floating-point type.
    #[error("mesh sample range is not representable as f32")]
    SampleRangeNotRepresentable,
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
        mesh_name: &'static str,
    ) -> Result<Self, PdfShadingError> {
        let color_input_count = if functions.is_empty() {
            color_space.num_color_components()
        } else {
            1
        };
        if color_input_count == 0 {
            return Err(MeshDecoderError::MissingColorComponents { mesh: mesh_name }.into());
        }

        let expected_values = color_input_count.saturating_add(2).saturating_mul(2);
        if decode.len() != expected_values {
            return Err(MeshDecoderError::InvalidDecodeLength {
                mesh: mesh_name,
                expected: expected_values,
                actual: decode.len(),
            }
            .into());
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
        let x = self.read_sample(reader, self.widths.coordinate(), x_min, x_max)?;
        let y = self.read_sample(reader, self.widths.coordinate(), y_min, y_max)?;
        Ok(Point::new(x, y))
    }

    /// Reads mesh color inputs, applies optional functions, and converts them
    /// into the shading color space.
    pub(crate) fn read_color(&self, reader: &mut BitReader<'_>) -> Result<Color, PdfShadingError> {
        let inputs = (0..self.color_input_count)
            .map(|component| {
                let (min, max) =
                    decode_pair(self.decode, component.saturating_add(2), "component")?;
                self.read_sample(reader, self.widths.component(), min, max)
            })
            .collect::<Result<Vec<_>, PdfShadingError>>()?;

        let components = self.apply_functions(&inputs)?;
        Ok(self.color_space.apply(&components)?)
    }

    /// Reads a required packed sample and maps it into the supplied decode range.
    ///
    /// The encoded integer is scaled linearly from the range representable by
    /// `width` bits into `min..=max`. An error is returned when the stream ends
    /// before the sample, contains only part of it, uses an unsupported width,
    /// or the encoded value or its range cannot be represented as `f32`.
    fn read_sample(
        &self,
        reader: &mut BitReader<'_>,
        width: usize,
        min: f32,
        max: f32,
    ) -> Result<f32, PdfShadingError> {
        let code = read_mesh_bits(reader, width)?
            .ok_or_else(|| PdfShadingError::from(MeshDecoderError::UnexpectedEndOfStream))?;
        let shift = u32::try_from(width)
            .map_err(|_| PdfShadingError::from(MeshDecoderError::InvalidBitFieldWidth { width }))?;
        let code_max = 1_u64
            .checked_shl(shift)
            .ok_or_else(|| PdfShadingError::from(MeshDecoderError::InvalidBitFieldWidth { width }))?
            .saturating_sub(1);
        let code = u64::from(code)
            .to_f32()
            .ok_or_else(|| PdfShadingError::from(MeshDecoderError::SampleNotRepresentable))?;
        let code_max = code_max
            .to_f32()
            .ok_or_else(|| PdfShadingError::from(MeshDecoderError::SampleRangeNotRepresentable))?;

        Ok(min + (code / code_max) * (max - min))
    }

    fn apply_functions(&self, inputs: &[f32]) -> Result<Vec<f32>, PdfShadingError> {
        match self.functions {
            [] => Ok(inputs.to_vec()),
            [function] => Ok(function.apply(inputs)?),
            functions => functions
                .iter()
                .map(|function| {
                    function.apply(inputs)?.first().copied().ok_or_else(|| {
                        PdfShadingError::from(MeshDecoderError::MissingFunctionColorComponent)
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
        return Err(MeshDecoderError::InvalidBitFieldWidth { width }.into());
    }
    let width = u8::try_from(width)
        .map_err(|_| PdfShadingError::from(MeshDecoderError::InvalidBitFieldWidth { width }))?;
    if reader.exhausted() {
        return Ok(None);
    }

    reader
        .read_bits_u32(width)
        .map(Some)
        .ok_or_else(|| PdfShadingError::from(MeshDecoderError::TruncatedSample))
}

fn decode_pair(
    decode: &[f32],
    pair_index: usize,
    label: &'static str,
) -> Result<(f32, f32), PdfShadingError> {
    let first_index = pair_index.saturating_mul(2);
    let second_index = first_index.saturating_add(1);
    let min = decode
        .get(first_index)
        .copied()
        .ok_or_else(|| PdfShadingError::from(MeshDecoderError::MissingDecodeMinimum { label }))?;
    let max = decode
        .get(second_index)
        .copied()
        .ok_or_else(|| PdfShadingError::from(MeshDecoderError::MissingDecodeMaximum { label }))?;
    Ok((min, max))
}

#[cfg(test)]
#[path = "../tests/mesh_decoder.rs"]
mod tests;
