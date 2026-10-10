//! Locating and adjusting the frame header of a JPEG stream.
//!
//! The frame header (`SOFn`) declares the number of lines in the image. Some
//! writers leave it at `0`, deferring it to a DNL marker, or at `0xFFFF` as a
//! placeholder for an unknown height. A decoder that trusts such a value reads
//! the scans as if they covered far more rows than they encode. A progressive
//! scan then consumes the following scans' data, and the decoded image keeps
//! only its coarsest coefficients.

/// Marker byte of a start-of-image marker.
const SOI: u8 = 0xD8;
/// Marker byte of an end-of-image marker.
const EOI: u8 = 0xD9;
/// Marker byte of a start-of-scan marker.
const SOS: u8 = 0xDA;
/// Marker byte of the temporary-use marker, which has no length field.
const TEM: u8 = 0x01;

/// Offset of the height field from the `0xFF` byte of a frame marker: the
/// marker (2 bytes), the segment length (2 bytes) and the sample precision
/// (1 byte) precede it.
const HEIGHT_FIELD_OFFSET: usize = 5;

/// The number of lines declared by a JPEG frame header and where it is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JpegFrameHeader {
    /// Byte offset of the big-endian height field within the stream.
    height_offset: usize,
    /// The declared number of lines.
    height: u16,
}

impl JpegFrameHeader {
    /// Finds the first frame header of `data` by walking the marker segments
    /// that follow the start-of-image marker.
    ///
    /// Returns `None` when `data` does not start with a start-of-image marker,
    /// when a scan or the end of the image precedes any frame header, or when a
    /// segment is truncated.
    pub(crate) fn find(data: &[u8]) -> Option<Self> {
        if data.get(..2)? != [0xFF, SOI] {
            return None;
        }

        let mut position = 2usize;
        loop {
            if *data.get(position)? != 0xFF {
                return None;
            }
            let marker = *data.get(position.checked_add(1)?)?;
            match marker {
                // Fill bytes may precede a marker.
                0xFF => position = position.checked_add(1)?,
                SOS | EOI => return None,
                TEM | 0xD0..=0xD7 => position = position.checked_add(2)?,
                0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => {
                    let height_offset = position.checked_add(HEIGHT_FIELD_OFFSET)?;
                    let bytes = data.get(height_offset..height_offset.checked_add(2)?)?;
                    return Some(Self {
                        height_offset,
                        height: u16::from_be_bytes(bytes.try_into().ok()?),
                    });
                }
                _ => {
                    let length_offset = position.checked_add(2)?;
                    let length = data.get(length_offset..length_offset.checked_add(2)?)?;
                    let length = u16::from_be_bytes(length.try_into().ok()?);
                    position = length_offset.checked_add(usize::from(length))?;
                }
            }
        }
    }

    /// Returns the height that a decoder should use instead of the declared
    /// one, given the image dictionary's `/Height`.
    ///
    /// A declared height of `0` (defined by a DNL marker) or `0xFFFF` (a
    /// placeholder) is replaced by a positive dictionary height smaller than
    /// it. Any other declared height is kept.
    pub(crate) fn replacement_height(&self, dictionary_height: Option<u16>) -> Option<u16> {
        let dictionary_height = dictionary_height.filter(|&height| height > 0)?;
        match self.height {
            0 => Some(dictionary_height),
            u16::MAX if dictionary_height < u16::MAX => Some(dictionary_height),
            _ => None,
        }
    }

    /// Returns a copy of `data` with this frame header declaring `height`
    /// lines.
    pub(crate) fn with_height(&self, data: &[u8], height: u16) -> Vec<u8> {
        let mut patched = data.to_vec();
        if let Some(field) = self
            .height_offset
            .checked_add(2)
            .and_then(|end| patched.get_mut(self.height_offset..end))
        {
            field.copy_from_slice(&height.to_be_bytes());
        }
        patched
    }
}
