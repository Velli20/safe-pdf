//! The JPX Reader Requirements box.
//!
//! Part 2 Annex M gives a JPX file a list of the features a reader must
//! understand to display it, each with a bit in the fully-understand and
//! decode-completely masks. The numbering of those standard flags is not
//! reproduced here, so the box is validated and reported rather than used to
//! accept or reject a file: the features this decoder cannot handle are refused
//! where they appear, by the box or marker that carries them.

use crate::{box_reader::Jp2Box, jp2::ContainerError};

/// Bytes in the `VF` field of a vendor feature record.
const VENDOR_UUID_BYTES: usize = 16;
/// Bytes in the `SF` field of a standard flag record.
const STANDARD_FLAG_BYTES: usize = 2;
/// Bytes in the `NSF` and `NVF` count fields.
const COUNT_BYTES: usize = 2;

/// A validated Reader Requirements box whose fields remain borrowed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReaderRequirements<'a> {
    mask_bytes: usize,
    fully_understand: &'a [u8],
    decode_completely: &'a [u8],
    standard: &'a [u8],
    vendor: &'a [u8],
}

impl<'a> TryFrom<Jp2Box<'a>> for ReaderRequirements<'a> {
    type Error = ContainerError;

    /// Reads `ML`, the two masks, and the standard and vendor feature lists.
    ///
    /// The mask width is the one Part 2 permits, and both lists must end
    /// exactly where the box does, so a truncated or padded box is refused
    /// instead of being read past its last record.
    fn try_from(box_view: Jp2Box<'a>) -> Result<Self, Self::Error> {
        let invalid = || ContainerError::from(box_view);
        let (mask_length, rest) = box_view.payload().split_first().ok_or_else(invalid)?;
        let mask_bytes = match *mask_length {
            1 | 2 | 4 | 8 => usize::from(*mask_length),
            _ => return Err(invalid()),
        };
        let (fully_understand, rest) = rest.split_at_checked(mask_bytes).ok_or_else(invalid)?;
        let (decode_completely, rest) = rest.split_at_checked(mask_bytes).ok_or_else(invalid)?;
        let record = STANDARD_FLAG_BYTES
            .checked_add(mask_bytes)
            .ok_or_else(invalid)?;
        let (standard, rest) = Self::take_records(rest, record).ok_or_else(invalid)?;
        let record = VENDOR_UUID_BYTES
            .checked_add(mask_bytes)
            .ok_or_else(invalid)?;
        let (vendor, rest) = Self::take_records(rest, record).ok_or_else(invalid)?;
        if !rest.is_empty() {
            return Err(invalid());
        }
        Ok(Self {
            mask_bytes,
            fully_understand,
            decode_completely,
            standard,
            vendor,
        })
    }
}

impl<'a> ReaderRequirements<'a> {
    /// Returns the mask naming the features a reader must fully understand.
    pub fn fully_understand_mask(&self) -> &'a [u8] {
        self.fully_understand
    }

    /// Returns the mask naming the features needed to decode completely.
    pub fn decode_completely_mask(&self) -> &'a [u8] {
        self.decode_completely
    }

    /// Returns the standard feature numbers the file declares, in file order.
    pub fn standard_flags(&self) -> impl Iterator<Item = u16> + 'a {
        let record = STANDARD_FLAG_BYTES.saturating_add(self.mask_bytes);
        self.standard
            .chunks_exact(record)
            .filter_map(|entry| entry.first_chunk::<STANDARD_FLAG_BYTES>().copied())
            .map(u16::from_be_bytes)
    }

    /// Returns the UUID of each vendor feature the file declares.
    pub fn vendor_features(&self) -> impl Iterator<Item = &'a [u8; VENDOR_UUID_BYTES]> + 'a {
        let record = VENDOR_UUID_BYTES.saturating_add(self.mask_bytes);
        self.vendor
            .chunks_exact(record)
            .filter_map(<[u8]>::first_chunk::<VENDOR_UUID_BYTES>)
    }

    /// Borrows a counted list of fixed-width records and the bytes after it.
    ///
    /// `record` is never zero, because both lists carry a count field wider
    /// than zero bytes alongside a mask of at least one byte.
    fn take_records(bytes: &'a [u8], record: usize) -> Option<(&'a [u8], &'a [u8])> {
        let (count, rest) = bytes.split_at_checked(COUNT_BYTES)?;
        let count = usize::from(u16::from_be_bytes(*count.first_chunk::<COUNT_BYTES>()?));
        rest.split_at_checked(count.checked_mul(record)?)
    }
}
