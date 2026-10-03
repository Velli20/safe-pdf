//! The JPX Codestream Registration box.
//!
//! Part 2 Annex M lets a compositing layer draw on several codestreams, each
//! registered on the layer's grid with its own sampling and offset. This
//! decoder reconstructs one codestream per image, so the box is read to learn
//! which codestream a layer names, and a registration that truly combines
//! several codestreams is refused by the caller rather than approximated.

use pdf_graphics::Size;

use crate::{box_reader::Jp2Box, jp2::ContainerError};

/// Bytes in the `XS` and `YS` grid fields that precede the records.
const GRID_BYTES: usize = 4;
/// Bytes in one codestream registration record.
const RECORD_BYTES: usize = 6;
/// Sampling and offset of a codestream that covers the layer grid exactly.
const UNIT_SAMPLING: u8 = 1;

/// One codestream registered with a compositing layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodestreamRegistrationRecord {
    /// Zero-based index of the codestream in the file's codestream order.
    pub codestream: u16,
    /// Horizontal sampling of this codestream on the layer grid.
    pub horizontal_sampling: u8,
    /// Vertical sampling of this codestream on the layer grid.
    pub vertical_sampling: u8,
    /// Horizontal offset of this codestream on the layer grid.
    pub horizontal_offset: u8,
    /// Vertical offset of this codestream on the layer grid.
    pub vertical_offset: u8,
}

impl CodestreamRegistrationRecord {
    /// Returns whether the codestream covers the layer grid one sample to one.
    pub(crate) fn covers_grid(self) -> bool {
        self.horizontal_sampling == UNIT_SAMPLING
            && self.vertical_sampling == UNIT_SAMPLING
            && self.horizontal_offset == 0
            && self.vertical_offset == 0
    }
}

/// A validated Codestream Registration box whose records remain borrowed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CodestreamRegistration<'a> {
    grid: Size<u16>,
    records: &'a [[u8; RECORD_BYTES]],
}

impl<'a> TryFrom<Jp2Box<'a>> for CodestreamRegistration<'a> {
    type Error = ContainerError;

    /// Reads the layer grid and checks that the records fill the box exactly.
    fn try_from(box_view: Jp2Box<'a>) -> Result<Self, Self::Error> {
        let invalid = || ContainerError::from(box_view);
        let (grid, body) = box_view
            .payload()
            .split_at_checked(GRID_BYTES)
            .ok_or_else(invalid)?;
        let [width_hi, width_lo, height_hi, height_lo] =
            *grid.first_chunk::<GRID_BYTES>().ok_or_else(invalid)?;
        let grid = Size {
            width: u16::from_be_bytes([width_hi, width_lo]),
            height: u16::from_be_bytes([height_hi, height_lo]),
        };
        let (records, remainder) = body.as_chunks::<RECORD_BYTES>();
        if grid.width == 0 || grid.height == 0 || records.is_empty() || !remainder.is_empty() {
            return Err(invalid());
        }
        Ok(Self { grid, records })
    }
}

impl CodestreamRegistration<'_> {
    /// Returns the grid the layer's codestreams are registered against.
    pub fn grid(&self) -> Size<u16> {
        self.grid
    }

    /// Returns every registration record in file order.
    pub fn iter(&self) -> impl Iterator<Item = CodestreamRegistrationRecord> + '_ {
        self.records.iter().map(
            |&[
                codestream_hi,
                codestream_lo,
                horizontal_sampling,
                vertical_sampling,
                horizontal_offset,
                vertical_offset,
            ]| {
                CodestreamRegistrationRecord {
                    codestream: u16::from_be_bytes([codestream_hi, codestream_lo]),
                    horizontal_sampling,
                    vertical_sampling,
                    horizontal_offset,
                    vertical_offset,
                }
            },
        )
    }

    /// Returns the single codestream this layer is registered with.
    ///
    /// `None` means the layer is composed from several codestreams, so its
    /// samples are not the samples of one reconstructed codestream.
    pub(crate) fn sole_codestream(&self) -> Option<CodestreamRegistrationRecord> {
        let [_] = self.records else { return None };
        self.iter().next()
    }
}
