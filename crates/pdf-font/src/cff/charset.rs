use crate::cff::cursor::{Cursor, CursorReadError};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur while reading / decoding a CFF charset.
#[derive(Debug, Error)]
pub enum CharsetError {
    #[error("Cursor read error: {0}")]
    CursorReadError(#[from] CursorReadError),
    #[error("Too many glyphs in charset (max 65535) got {0}")]
    TooManyGlyphs(usize),
    #[error("Unsupported charset format: {0}")]
    UnsupportedCharsetFormat(u8),
    #[error("Supplemental encoding not supported")]
    SupplementalEncodingNotSupported,
}

/// Represents a CFF charset, mapping SIDs to GIDs.
pub struct Charset {
    map: HashMap<u16, u16>,
}

impl Charset {
    /// Reads a CFF charset from the supplied cursor.
    ///
    /// # Parameters
    ///
    /// - `cur`: The cursor positioned at the start of the charset data.
    /// - `number_of_glyphs`: The total number of glyphs in the font
    ///
    /// # Returns
    ///
    /// A [`Charset`] providing SID → GID reverse lookup.
    pub fn read<'a>(
        cur: &mut Cursor<'a>,
        number_of_glyphs: usize,
    ) -> Result<Charset, CharsetError> {
        let format = cur.read_u8()?;
        // The first high-bit in format indicates that a Supplemental encoding is present.
        let has_supplemental = format & 0x80 != 0;
        if has_supplemental {
            return Err(CharsetError::SupplementalEncodingNotSupported);
        }

        let number_of_glyphs = u16::try_from(number_of_glyphs)
            .or(Err(CharsetError::TooManyGlyphs(number_of_glyphs)))?;

        // Clear the high-bit to get the actual format number.
        let format = format & 0x7f;
        match format {
            0 => {
                let mut sids = HashMap::new();
                // Map .notdef (SID 0) to GID 0
                sids.insert(0, 0);
                for i in 1..number_of_glyphs {
                    let sid = cur.read_u16()?;
                    sids.insert(sid, i);
                }
                Ok(Charset { map: sids })
            }

            _ => Err(CharsetError::UnsupportedCharsetFormat(format)),
        }
    }

    /// Looks up the GID (glyph index) for a given SID.
    ///
    /// # Parameters
    ///
    /// - `sid`: The String Identifier to look up.
    ///
    /// # Returns
    ///
    /// The GID (glyph index) corresponding to the given SID, or `None` if not found.
    #[inline]
    pub fn get_gid(&self, sid: u16) -> Option<u16> {
        self.map.get(&sid).copied()
    }
}
