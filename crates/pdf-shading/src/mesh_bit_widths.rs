//! Parsing and validation of packed mesh bit widths.

use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::{error::PdfShadingError, mesh_decoder::MeshDecoderError};

/// Coordinate widths permitted by the PDF specification for `/BitsPerCoordinate`.
const VALID_COORDINATE_WIDTHS: [usize; 8] = [1, 2, 4, 8, 12, 16, 24, 32];

/// Color component widths permitted by the PDF specification for `/BitsPerComponent`.
const VALID_COMPONENT_WIDTHS: [usize; 6] = [1, 2, 4, 8, 12, 16];

/// Edge-flag widths permitted by the PDF specification for `/BitsPerFlag`.
const VALID_FLAG_WIDTHS: [usize; 3] = [2, 4, 8];

/// Bit widths used to decode packed fields in a Type 4, 6, or 7 mesh stream.
///
/// Each value is read from the corresponding mesh shading dictionary entry
/// and validated against the widths permitted by the PDF specification.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MeshBitWidths {
    /// Number of bits used to encode each coordinate component.
    coordinate: usize,
    /// Number of bits used to encode each color component or function input.
    component: usize,
    /// Number of bits used to encode each vertex or patch edge flag.
    flag: usize,
}

/// Reads and validates the bit widths declared by a mesh shading dictionary.
///
/// The required `/BitsPerCoordinate`, `/BitsPerComponent`, and `/BitsPerFlag` entries are
/// checked against the widths permitted by the PDF specification.
impl FromPdfObject for MeshBitWidths {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let coordinate = context.required::<usize>(b"BitsPerCoordinate")?;
        let component = context.required::<usize>(b"BitsPerComponent")?;
        let flag = context.required::<usize>(b"BitsPerFlag")?;

        validate_allowed_width(
            coordinate,
            &VALID_COORDINATE_WIDTHS,
            MeshDecoderError::InvalidBitsPerCoordinate { value: coordinate },
        )?;
        validate_allowed_width(
            component,
            &VALID_COMPONENT_WIDTHS,
            MeshDecoderError::InvalidBitsPerComponent { value: component },
        )?;
        validate_allowed_width(
            flag,
            &VALID_FLAG_WIDTHS,
            MeshDecoderError::InvalidBitsPerFlag { value: flag },
        )?;

        Ok(Self {
            coordinate,
            component,
            flag,
        })
    }
}

impl MeshBitWidths {
    /// Returns the number of bits used to encode each coordinate component.
    pub(crate) fn coordinate(self) -> usize {
        self.coordinate
    }

    /// Returns the number of bits used to encode each color component or function input.
    pub(crate) fn component(self) -> usize {
        self.component
    }

    /// Returns the number of bits used to encode each vertex or patch edge flag.
    pub(crate) fn flag(self) -> usize {
        self.flag
    }
}

/// Checks whether a mesh field width is one of the values permitted for that field.
///
/// Returns `Ok(())` when `width` appears in `allowed`; otherwise converts the
/// field-specific `error` into [`PdfShadingError`].
fn validate_allowed_width(
    width: usize,
    allowed: &[usize],
    error: MeshDecoderError,
) -> Result<(), PdfShadingError> {
    if allowed.contains(&width) {
        Ok(())
    } else {
        Err(error.into())
    }
}

#[cfg(test)]
#[path = "../tests/mesh_bit_widths.rs"]
mod tests;
