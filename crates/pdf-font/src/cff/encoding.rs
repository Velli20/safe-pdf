use crate::cff::charset::Charset;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur while reading / decoding a CFF charset.
#[derive(Debug, Error)]
pub enum EncodingError {
    #[error("Unsupported encoding type: {0}, only StandardEncoding (0) is supported")]
    UnsupportedEncodingType(u16),
}

/// Represents a CFF encoding, mapping 8‑bit character codes to glyph IDs (GIDs).
pub enum Encoding {
    /// Represents the predefined StandardEncoding mapping.
    Standard(HashMap<u16, u16>),
}

/// Predefined StandardEncoding table mapping 8‑bit character codes to SIDs.
///
/// A value of `0` in this table indicates either:
/// - the code position is unused / unassigned in StandardEncoding, or
/// - the SID is 0 which by definition is `.notdef`.
#[rustfmt::skip]
pub const STANDARD_ENCODING: [u8; 256] = [
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      1,   2,   3,   4,   5,   6,   7,   8,   9,  10,  11,  12,  13,  14,  15,  16,
     17,  18,  19,  20,  21,  22,  23,  24,  25,  26,  27,  28,  29,  30,  31,  32,
     33,  34,  35,  36,  37,  38,  39,  40,  41,  42,  43,  44,  45,  46,  47,  48,
     49,  50,  51,  52,  53,  54,  55,  56,  57,  58,  59,  60,  61,  62,  63,  64,
     65,  66,  67,  68,  69,  70,  71,  72,  73,  74,  75,  76,  77,  78,  79,  80,
     81,  82,  83,  84,  85,  86,  87,  88,  89,  90,  91,  92,  93,  94,  95,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0,  96,  97,  98,  99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110,
      0, 111, 112, 113, 114,   0, 115, 116, 117, 118, 119, 120, 121, 122,   0, 123,
      0, 124, 125, 126, 127, 128, 129, 130, 131,   0, 132, 133,   0, 134, 135, 136,
    137,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,   0,
      0, 138,   0, 139,   0,   0,   0,   0, 140, 141, 142, 143,   0,   0,   0,   0,
      0, 144,   0,   0,   0, 145,   0,   0, 146, 147, 148, 149,   0,   0,   0,   0,
];

impl Encoding {
    /// Constructs an `Encoding` from a CFF `Charset`.
    ///
    /// # Parameters
    ///
    /// - `charset`: The CFF `Charset` to use for SID → GID resolution.
    /// - `dictionary_offset`: The offset in the font file where the encoding data starts.
    ///
    /// # Returns
    ///
    /// A Result containing the `Encoding` or an `EncodingError` if the encoding type is unsupported.
    pub fn from_charset(charset: &Charset, dictionary_offset: u16) -> Result<Self, EncodingError> {
        if dictionary_offset != 0 {
            return Err(EncodingError::UnsupportedEncodingType(dictionary_offset));
        }

        let mut mapping = HashMap::new();
        const NOTDEF_GID: u16 = 0u16;
        for code_point in 0u16..=255 {
            let sid = STANDARD_ENCODING[usize::from(code_point)];
            let gid = charset.get_gid(u16::from(sid)).unwrap_or(NOTDEF_GID);
            mapping.insert(code_point, gid);
        }
        Ok(Encoding::Standard(mapping))
    }

    /// Returns the glyph ID (GID) associated with a character code.
    ///
    /// # Parameters
    ///
    /// - `code`: The unicode character code to look up.
    #[inline]
    pub fn gid(&self, code: u16) -> Option<u16> {
        match self {
            Encoding::Standard(map) => map.get(&code).copied(),
        }
    }
}
