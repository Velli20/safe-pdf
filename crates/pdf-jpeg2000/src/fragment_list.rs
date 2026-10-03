//! The JPX Fragment List box inside a Fragment Table.
//!
//! Part 2 Annex M lets a codestream be stored as fragments, each naming an
//! offset, a length, and the file it lives in. Every stage above the container
//! reads the codestream as one borrowed slice, so a fragment list is usable
//! here only when its fragments lie in this file, in order, and end where the
//! next one begins. The caller refuses the other arrangements by name.

use core::ops::Range;

use crate::{Jpeg2000Error, box_reader::Jp2Box, jp2::ContainerError};

/// Bytes in one fragment list record.
const RECORD_BYTES: usize = 14;
/// `DR` value naming the file that carries the Fragment List box itself.
const THIS_FILE: u16 = 0;

/// One fragment of a codestream stored outside a Contiguous Codestream box.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fragment {
    /// Byte offset of the fragment within the file that carries it.
    pub offset: u64,
    /// Byte length of the fragment.
    pub length: u32,
    /// One-based Data Reference index, or zero for this file.
    pub data_reference: u16,
}

/// Where the fragments of one codestream lie.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FragmentLayout {
    /// The fragments form one range of this file.
    Contiguous(Range<usize>),
    /// At least one fragment lives in a file named by a Data Reference box.
    External,
    /// The fragments lie in this file but do not form one ascending range.
    Scattered,
}

/// A validated Fragment List box whose records remain borrowed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FragmentList<'a> {
    records: &'a [[u8; RECORD_BYTES]],
}

impl<'a> TryFrom<Jp2Box<'a>> for FragmentList<'a> {
    type Error = ContainerError;

    /// Checks the declared fragment count against the payload length.
    fn try_from(box_view: Jp2Box<'a>) -> Result<Self, Self::Error> {
        let [count_hi, count_lo, body @ ..] = box_view.payload() else {
            return Err(box_view.into());
        };
        let count: usize = u16::from_be_bytes([*count_hi, *count_lo]).into();
        let (records, remainder) = body.as_chunks::<RECORD_BYTES>();
        if count == 0 || count != records.len() || !remainder.is_empty() {
            return Err(box_view.into());
        }
        let list = Self { records };
        if list.iter().any(|fragment| fragment.length == 0) {
            return Err(box_view.into());
        }
        Ok(list)
    }
}

impl FragmentList<'_> {
    /// Returns every fragment in the order the codestream needs them.
    pub fn iter(&self) -> impl Iterator<Item = Fragment> + '_ {
        self.records.iter().map(
            |&[
                offset_a,
                offset_b,
                offset_c,
                offset_d,
                offset_e,
                offset_f,
                offset_g,
                offset_h,
                length_a,
                length_b,
                length_c,
                length_d,
                reference_hi,
                reference_lo,
            ]| Fragment {
                offset: u64::from_be_bytes([
                    offset_a, offset_b, offset_c, offset_d, offset_e, offset_f, offset_g, offset_h,
                ]),
                length: u32::from_be_bytes([length_a, length_b, length_c, length_d]),
                data_reference: u16::from_be_bytes([reference_hi, reference_lo]),
            },
        )
    }

    /// Returns where the listed fragments lie relative to this file.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` when a declared offset or length is not addressable
    /// on this target.
    pub(crate) fn layout(&self) -> Result<FragmentLayout, Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "JPX fragment offset",
        };
        let mut range: Option<Range<u64>> = None;
        for fragment in self.iter() {
            if fragment.data_reference != THIS_FILE {
                return Ok(FragmentLayout::External);
            }
            let end = fragment
                .offset
                .checked_add(u64::from(fragment.length))
                .ok_or_else(overflow)?;
            match range {
                Some(ref mut extent) if extent.end == fragment.offset => extent.end = end,
                Some(_) => return Ok(FragmentLayout::Scattered),
                None => range = Some(fragment.offset..end),
            }
        }
        let extent = range.ok_or_else(overflow)?;
        let start = usize::try_from(extent.start).map_err(|_| overflow())?;
        let end = usize::try_from(extent.end).map_err(|_| overflow())?;
        Ok(FragmentLayout::Contiguous(start..end))
    }
}
