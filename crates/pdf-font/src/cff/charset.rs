//! CFF (Compact Font Format) charset parsing.
//!
//! A CFF font uses a *charset* table to map glyph indices (GIDs) to
//! String Identifiers (SIDs). The SID space references entries in the
//! CFF String INDEX (standard + local strings). The relationship is:
//!
//! ```text
//! GID 0  -> implicit .notdef (SID 0)
//! GID 1  -> first glyph after .notdef
//! GID n  -> nth glyph as defined by the charset data
//! ```
//!
//! This module currently implements only charset Format 0 as defined in the
//! CFF specification (Adobe Technical Note #5176 / OpenType CFF table). Format 0
//! stores an explicit 2‑byte SID for every glyph after GID 0.
//!
//! Supplemental encodings (high-bit set in the format byte) and Formats 1 & 2
//! (range based encodings) are not yet supported. Attempting to read such data
//! results in an error (or a panic for supplemental encodings until a dedicated
//! error variant is added).
//!
//! The `Charset` structure produced here is a reverse lookup map from SID → GID.
//! This direction is convenient when resolving a glyph by its String ID while
//! interpreting CharStrings. If a future need arises we can add a forward map
//! or derive it on demand.
//!
//! # Future Work
//! - Support formats 1 & 2 (range encodings) to reduce memory for contiguous SIDs.

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
                    // Each SID is stored as a big-endian u16
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
