//! The JP2 Palette box and its indexed lookup table.
//!
//! Part 1 I.5.3.4 stores a palette as a table of `NE` entries, each holding
//! `NPC` samples whose widths come from the box's depth bytes. The samples
//! stay borrowed in the caller's slice and are read one at a time, so a
//! hostile entry count cannot force an allocation.

use crate::{box_reader::Jp2Box, jp2::ContainerError, size::MAX_PRECISION};

/// Bytes preceding the depth bytes of a Palette box.
const HEADER_BYTES: usize = 3;
/// Bits of a depth byte holding one less than the sample precision.
const PRECISION_MASK: u8 = 0x7f;
/// Bit of a depth byte marking the column as signed.
const SIGNED_FLAG: u8 = 0x80;
/// Palette samples are stored in whole bytes.
const BITS_PER_BYTE: u8 = 8;

/// Width and signedness of one palette column.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaletteColumn {
    /// Sample precision in bits, between 1 and 38.
    pub precision: u8,
    /// Whether the column's samples are signed.
    pub signed: bool,
}

impl From<&u8> for PaletteColumn {
    /// Splits a `Bi` depth byte into its precision and signedness.
    fn from(depth: &u8) -> Self {
        Self {
            precision: (depth & PRECISION_MASK).saturating_add(1),
            signed: depth & SIGNED_FLAG != 0,
        }
    }
}

impl PaletteColumn {
    /// Returns the number of stored bytes one sample of this column occupies.
    fn stored_bytes(self) -> usize {
        self.precision.div_ceil(BITS_PER_BYTE).into()
    }

    /// Reads one big-endian sample of this column and restores its sign.
    ///
    /// Part 1 stores a signed palette sample in two's complement within its
    /// declared precision, so the value is sign-extended from that width
    /// rather than from the padded byte width.
    fn read(self, stored: &[u8]) -> Option<i64> {
        let raw = stored.iter().try_fold(0u64, |value, byte| {
            value
                .checked_shl(BITS_PER_BYTE.into())?
                .checked_add((*byte).into())
        })?;
        let value = i64::try_from(raw).ok()?;
        if !self.signed {
            return Some(value);
        }
        let span = 1i64.checked_shl(self.precision.into())?;
        let sign_bit = span.checked_div(2)?;
        if value < sign_bit {
            return Some(value);
        }
        value.checked_sub(span)
    }
}

/// A validated JP2 Palette box whose sample table remains borrowed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Palette<'a> {
    depths: &'a [u8],
    samples: &'a [u8],
    entries: u16,
    row_bytes: usize,
}

impl<'a> TryFrom<Jp2Box<'a>> for Palette<'a> {
    type Error = ContainerError;

    /// Validates the entry count, column depths, and sample table length.
    fn try_from(box_view: Jp2Box<'a>) -> Result<Self, Self::Error> {
        let Some((header, rest)) = box_view.payload().split_at_checked(HEADER_BYTES) else {
            return Err(box_view.into());
        };
        let [entry_hi, entry_lo, columns] = header else {
            return Err(box_view.into());
        };
        let entries = u16::from_be_bytes([*entry_hi, *entry_lo]);
        let (depths, samples) = rest.split_at_checked((*columns).into()).ok_or(box_view)?;
        let row_bytes = depths
            .iter()
            .map(PaletteColumn::from)
            .try_fold(0usize, |width, column| {
                if column.precision > MAX_PRECISION {
                    return None;
                }
                width.checked_add(column.stored_bytes())
            })
            .ok_or(box_view)?;
        let table_bytes = row_bytes.checked_mul(entries.into()).ok_or(box_view)?;
        if entries == 0 || *columns == 0 || samples.len() != table_bytes {
            return Err(box_view.into());
        }
        Ok(Self {
            depths,
            samples,
            entries,
            row_bytes,
        })
    }
}

impl<'a> Palette<'a> {
    /// Returns the number of entries the palette can be indexed by.
    pub fn entry_count(&self) -> u16 {
        self.entries
    }

    /// Returns the number of channels one palette entry produces.
    pub fn column_count(&self) -> u8 {
        // A Palette box carries at most 255 depth bytes, so the length fits.
        u8::try_from(self.depths.len()).unwrap_or(u8::MAX)
    }

    /// Returns the width and signedness of one palette column.
    pub fn column(&self, index: u8) -> Option<PaletteColumn> {
        self.depths.get(usize::from(index)).map(PaletteColumn::from)
    }

    /// Returns every column's width and signedness in storage order.
    pub fn columns(&self) -> impl Iterator<Item = PaletteColumn> + 'a {
        self.depths.iter().map(PaletteColumn::from)
    }

    /// Looks one entry up in one column, sign-extended to the full range.
    ///
    /// Returns `None` when either index is outside the table, which lets a
    /// caller report an out-of-range palette index from the codestream
    /// without conflating it with a malformed box.
    pub fn sample(&self, entry: u16, column: u8) -> Option<i64> {
        if entry >= self.entries {
            return None;
        }
        let layout = self.column(column)?;
        let row = self.row_bytes.checked_mul(entry.into())?;
        let within = self
            .depths
            .get(..usize::from(column))?
            .iter()
            .map(PaletteColumn::from)
            .try_fold(0usize, |offset, earlier| {
                offset.checked_add(earlier.stored_bytes())
            })?;
        let start = row.checked_add(within)?;
        let end = start.checked_add(layout.stored_bytes())?;
        layout.read(self.samples.get(start..end)?)
    }
}
