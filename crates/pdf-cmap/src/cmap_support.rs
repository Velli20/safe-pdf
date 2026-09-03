/// Convert a big-endian byte slice into a `u32`.
///
/// # Parameters
///
/// - `bytes`: The big-endian byte slice to pack into a `u32`.
///
/// # Returns
///
/// The packed integer value formed by shifting in each byte from left to
/// right.
pub(crate) fn bytes_to_u32(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |value, byte| {
        value.checked_shl(8).unwrap_or(0) | u32::from(*byte)
    })
}

/// Decode the first source code by matching the longest valid character code.
///
/// # Parameters
///
/// - `text`: Raw source bytes to decode.
/// - `allowed_code_lengths`: Source-code byte lengths to try, in ascending
///   order.
/// - `has_code_space`: Callback that checks whether a packed source code is
///   valid for a given byte length.
/// - `map_code`: Callback that maps one packed source code to a CID.
///
/// # Returns
///
/// The packed source code, consumed byte length, and mapped CID. Unknown or unmapped input returns
/// the first byte with CID 0 so a streaming caller can advance safely. Empty input returns `None`.
pub(crate) fn decode_next_with_code_space<HasCodeSpace, MapCode>(
    text: &[u8],
    allowed_code_lengths: &[usize],
    mut has_code_space: HasCodeSpace,
    mut map_code: MapCode,
) -> Option<(u32, usize, u16)>
where
    HasCodeSpace: FnMut(u32, usize) -> bool,
    MapCode: FnMut(u32) -> Option<u16>,
{
    let first = text.first().copied()?;
    for len in allowed_code_lengths.iter().rev() {
        let Some(bytes) = text.get(..*len) else {
            continue;
        };

        let code = bytes_to_u32(bytes);
        if has_code_space(code, *len) {
            return Some((code, *len, map_code(code).unwrap_or(0)));
        }
    }
    Some((u32::from(first), 1, 0))
}

/// Shared lookup contract for Type0 encoding CMaps.
pub(crate) trait Type0CodeMap {
    /// Return all source-code byte lengths accepted by this CMap.
    ///
    /// # Parameters
    ///
    /// - `self`: The CMap instance being queried.
    ///
    /// # Returns
    ///
    /// All accepted source-code byte lengths, sorted in ascending order.
    fn allowed_code_lengths(&self) -> &[usize];

    /// Return whether a packed source code is valid for a byte length.
    ///
    /// # Parameters
    ///
    /// - `self`: The CMap instance being queried.
    /// - `code`: The packed big-endian source code.
    /// - `len`: The number of bytes used to encode `code`.
    ///
    /// # Returns
    ///
    /// `true` when `code` is valid for the given byte length, otherwise
    /// `false`.
    fn has_code_space(&self, code: u32, len: usize) -> bool;

    /// Map one packed source code to its CID.
    ///
    /// # Parameters
    ///
    /// - `self`: The CMap instance being queried.
    /// - `code`: The packed big-endian source code.
    ///
    /// # Returns
    ///
    /// The mapped CID when the code is known, or `None` when no mapping
    /// exists.
    fn map_code_to_cid(&self, code: u32) -> Option<u16>;

    /// Decode the first source code using this CMap.
    ///
    /// # Parameters
    ///
    /// - `self`: The CMap used for decoding.
    /// - `text`: Raw source bytes to decode.
    ///
    /// # Returns
    ///
    /// The source code, consumed length, and CID selected by the longest valid code-space match.
    /// Non-empty input always consumes at least one byte, including malformed or unmapped input, so
    /// repeated calls cannot stall a streaming decoder.
    fn decode_next(&self, text: &[u8]) -> Option<(u32, usize, u16)> {
        decode_next_with_code_space(
            text,
            self.allowed_code_lengths(),
            |code, len| self.has_code_space(code, len),
            |code| self.map_code_to_cid(code),
        )
    }
}
