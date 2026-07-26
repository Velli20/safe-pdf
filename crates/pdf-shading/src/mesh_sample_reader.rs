//! Bit-level reading for packed PDF mesh samples.

use crate::error::PdfShadingError;

/// Reads unsigned, big-endian bit fields from a mesh shading stream.
///
/// The reader deliberately does not know the meaning of individual fields.
/// Mesh-specific parsers decide when record padding requires byte alignment.
pub(crate) struct MeshSampleReader<'a> {
    data: &'a [u8],
    bit_offset: usize,
}

impl<'a> MeshSampleReader<'a> {
    /// Creates a reader positioned at the first bit of `data`.
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            bit_offset: 0,
        }
    }

    /// Reads a field, returning `None` only when already at end-of-stream.
    pub(crate) fn read_bits(&mut self, width: usize) -> Result<Option<u32>, PdfShadingError> {
        validate_width(width)?;

        let available_bits = self.data.len().saturating_mul(8);
        if self.bit_offset >= available_bits {
            return Ok(None);
        }

        let end_offset = self.bit_offset.saturating_add(width);
        if end_offset > available_bits {
            return Err(invalid_mesh_data(
                "Mesh stream ended in the middle of a sample",
            ));
        }

        let mut value = 0_u32;
        for absolute_bit in self.bit_offset..end_offset {
            let byte_index = absolute_bit / 8;
            let bit_in_byte = absolute_bit % 8;
            let byte = self.data.get(byte_index).copied().ok_or_else(|| {
                invalid_mesh_data("Mesh stream byte access exceeded the available data")
            })?;
            let shift = 7_usize.saturating_sub(bit_in_byte);
            value = (value << 1) | u32::from((byte >> shift) & 1);
        }

        self.bit_offset = end_offset;
        Ok(Some(value))
    }

    /// Reads a required field and reports a truncated mesh stream at EOF.
    pub(crate) fn read_required_bits(&mut self, width: usize) -> Result<u32, PdfShadingError> {
        self.read_bits(width)?
            .ok_or_else(|| invalid_mesh_data("Mesh stream ended unexpectedly"))
    }

    /// Skips padding at the end of a byte-aligned mesh record.
    pub(crate) fn align_to_byte(&mut self) {
        self.bit_offset = self.bit_offset.div_ceil(8).saturating_mul(8);
    }
}

fn validate_width(width: usize) -> Result<(), PdfShadingError> {
    if (1..=32).contains(&width) {
        Ok(())
    } else {
        Err(invalid_mesh_data("Mesh bit-field widths must be in 1..=32"))
    }
}

fn invalid_mesh_data(reason: &str) -> PdfShadingError {
    PdfShadingError::InvalidShadingMeshData {
        reason: reason.to_string(),
    }
}

#[cfg(test)]
#[path = "../tests/mesh_sample_reader.rs"]
mod tests;
