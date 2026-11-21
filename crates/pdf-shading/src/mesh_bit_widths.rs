//! Parsing and validation of packed mesh bit widths.

use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::{error::PdfShadingError, mesh_decoder::MeshDecoderError};

/// Coordinate widths permitted by the PDF specification for `/BitsPerCoordinate`.
const VALID_COORDINATE_WIDTHS: [u8; 8] = [1, 2, 4, 8, 12, 16, 24, 32];

/// Color component widths permitted by the PDF specification for `/BitsPerComponent`.
const VALID_COMPONENT_WIDTHS: [u8; 6] = [1, 2, 4, 8, 12, 16];

/// Edge-flag widths permitted by the PDF specification for `/BitsPerFlag`.
const VALID_FLAG_WIDTHS: [u8; 3] = [2, 4, 8];

/// Bit widths used to decode packed fields in a Type 4, 6, or 7 mesh stream.
///
/// Every width is one of the values the PDF specification permits for its field.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MeshBitWidths {
    /// Number of bits used to encode each coordinate component.
    pub(crate) coordinate: u8,
    /// Number of bits used to encode each color component or function input.
    pub(crate) component: u8,
    /// Number of bits used to encode each vertex or patch edge flag.
    pub(crate) flag: u8,
}

/// Reads the required `/BitsPerCoordinate`, `/BitsPerComponent`, and `/BitsPerFlag` entries.
impl FromPdfObject for MeshBitWidths {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        Ok(Self::new(
            context.required(b"BitsPerCoordinate")?,
            context.required(b"BitsPerComponent")?,
            context.required(b"BitsPerFlag")?,
        )?)
    }
}

impl MeshBitWidths {
    /// Validates each width against the values permitted for its field.
    fn new(coordinate: usize, component: usize, flag: usize) -> Result<Self, PdfShadingError> {
        let find = |allowed: &[u8], value| {
            allowed
                .iter()
                .copied()
                .find(|&width| usize::from(width) == value)
        };
        Ok(Self {
            coordinate: find(&VALID_COORDINATE_WIDTHS, coordinate)
                .ok_or(MeshDecoderError::InvalidBitsPerCoordinate { value: coordinate })?,
            component: find(&VALID_COMPONENT_WIDTHS, component)
                .ok_or(MeshDecoderError::InvalidBitsPerComponent { value: component })?,
            flag: find(&VALID_FLAG_WIDTHS, flag)
                .ok_or(MeshDecoderError::InvalidBitsPerFlag { value: flag })?,
        })
    }
}

#[cfg(test)]
#[path = "../tests/mesh_bit_widths.rs"]
mod tests;
