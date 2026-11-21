//! Decoding of coordinates, colors, and flags from packed mesh samples.

use pdf_color_space::color_space::ColorSpace;
use pdf_decode::DecodeRange;
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
    /// The stream ended after only part of a packed sample was available.
    #[error("mesh stream ended in the middle of a sample")]
    TruncatedSample,
    /// The stream ended before a required sample began.
    #[error("mesh stream ended unexpectedly")]
    UnexpectedEndOfStream,
}

/// Decodes the packed records of a mesh stream according to its `/Decode` array.
///
/// The decoder owns the stream cursor and turns packed fields into flags,
/// points, and colors; the mesh parsers only reconstruct triangles and
/// patches from those values.
pub(crate) struct MeshDecoder<'a> {
    reader: MeshBitReader<'a>,
    widths: MeshBitWidths,
    x: DecodeRange,
    y: DecodeRange,
    /// One range per color component, or a single range for the function input.
    components: Vec<DecodeRange>,
    functions: &'a [Function],
    color_space: &'a ColorSpace,
}

impl<'a> MeshDecoder<'a> {
    /// Creates a decoder after validating `/Decode` against the color inputs.
    pub(crate) fn new(
        data: &'a [u8],
        widths: MeshBitWidths,
        decode: &[f32],
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

        let (x, y, components) = match decode.as_chunks::<2>() {
            ([x, y, components @ ..], []) if components.len() == color_input_count => {
                (x, y, components)
            }
            _ => {
                return Err(MeshDecoderError::InvalidDecodeLength {
                    mesh: mesh_name,
                    expected: color_input_count.saturating_add(2).saturating_mul(2),
                    actual: decode.len(),
                }
                .into());
            }
        };
        let range = |&[min, max]: &[f32; 2]| DecodeRange::new(min, max);

        Ok(Self {
            reader: MeshBitReader(BitReader::new(data)),
            widths,
            x: range(x)?,
            y: range(y)?,
            components: components.iter().map(range).collect::<Result<_, _>>()?,
            functions,
            color_space,
        })
    }

    /// Reads the next record's flag, or `None` when the stream ends cleanly.
    ///
    /// Only the two low bits carry meaning; the PDF format ignores the rest.
    pub(crate) fn read_flag(&mut self) -> Result<Option<u8>, PdfShadingError> {
        let flag = self.reader.read_field(self.widths.flag)?;
        Ok(flag.map(|value| {
            let [low, ..] = value.to_le_bytes();
            low & 0b11
        }))
    }

    /// Reads and decodes one `(x, y)` coordinate pair.
    pub(crate) fn read_point(&mut self) -> Result<Point, PdfShadingError> {
        let width = self.widths.coordinate;
        let x = self.reader.read_sample(width, self.x)?;
        let y = self.reader.read_sample(width, self.y)?;
        Ok(Point::new(x, y))
    }

    /// Reads mesh color inputs, applies optional functions, and converts them
    /// into the shading color space.
    pub(crate) fn read_color(&mut self) -> Result<Color, PdfShadingError> {
        let width = self.widths.component;
        let inputs = self
            .components
            .iter()
            .map(|&range| self.reader.read_sample(width, range))
            .collect::<Result<Vec<_>, _>>()?;

        let components = self.apply_functions(&inputs)?;
        Ok(self.color_space.apply(&components)?)
    }

    /// Skips the padding that ends a byte-aligned record.
    pub(crate) fn align_to_byte(&mut self) {
        self.reader.0.align_to_byte_boundary();
    }

    /// Maps decoded inputs to color components through the shading functions.
    fn apply_functions(&self, inputs: &[f32]) -> Result<Vec<f32>, PdfShadingError> {
        match self.functions {
            [] => Ok(inputs.to_vec()),
            [function] => Ok(function.apply(inputs)?),
            functions => functions
                .iter()
                .map(|function| {
                    let output = function.apply(inputs)?;
                    Ok(*output
                        .first()
                        .ok_or(MeshDecoderError::MissingFunctionColorComponent)?)
                })
                .collect(),
        }
    }
}

/// Reads fixed-width fields from a packed mesh stream.
struct MeshBitReader<'a>(BitReader<'a>);

impl MeshBitReader<'_> {
    /// Reads a field, distinguishing a clean end of stream from truncation.
    fn read_field(&mut self, width: u8) -> Result<Option<u32>, MeshDecoderError> {
        if self.0.exhausted() {
            return Ok(None);
        }
        self.0
            .read_bits_u32(width)
            .map(Some)
            .ok_or(MeshDecoderError::TruncatedSample)
    }

    /// Reads a required field and maps it into `range`.
    fn read_sample(&mut self, width: u8, range: DecodeRange) -> Result<f32, PdfShadingError> {
        let code = self
            .read_field(width)?
            .ok_or(MeshDecoderError::UnexpectedEndOfStream)?;
        Ok(range.map_code(code, width)?)
    }
}

#[cfg(test)]
#[path = "../tests/mesh_decoder.rs"]
mod tests;
