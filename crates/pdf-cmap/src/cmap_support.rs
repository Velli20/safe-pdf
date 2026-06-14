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

/// Decode text by matching the longest valid character code at each position.
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
/// A vector of decoded CIDs. Unknown or unmapped bytes are replaced with `0`
/// and decoding advances by one byte.
pub(crate) fn decode_with_code_space<HasCodeSpace, MapCode>(
    text: &[u8],
    allowed_code_lengths: &[usize],
    mut has_code_space: HasCodeSpace,
    mut map_code: MapCode,
) -> Vec<u16>
where
    HasCodeSpace: FnMut(u32, usize) -> bool,
    MapCode: FnMut(u32) -> Option<u16>,
{
    let mut decoded = Vec::new();
    let mut position = 0usize;

    while position < text.len() {
        let mut matched = None;

        for len in allowed_code_lengths.iter().rev() {
            let end = position.saturating_add(*len);
            let Some(bytes) = text.get(position..end) else {
                continue;
            };

            let code = bytes_to_u32(bytes);
            if has_code_space(code, *len) {
                matched = Some((*len, map_code(code).unwrap_or(0)));
                break;
            }
        }

        if let Some((len, cid)) = matched {
            decoded.push(cid);
            position = position.saturating_add(len);
        } else {
            decoded.push(0);
            position = position.saturating_add(1);
        }
    }

    decoded
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
    fn allowed_code_lengths(&self) -> Vec<usize>;

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

    /// Decode raw text bytes into CIDs using this CMap.
    ///
    /// # Parameters
    ///
    /// - `self`: The CMap used for decoding.
    /// - `text`: Raw source bytes to decode.
    ///
    /// # Returns
    ///
    /// The decoded CIDs produced by matching the longest valid source-code
    /// length at each position.
    fn decode_text(&self, text: &[u8]) -> Vec<u16> {
        let lengths = self.allowed_code_lengths();
        decode_with_code_space(
            text,
            &lengths,
            |code, len| self.has_code_space(code, len),
            |code| self.map_code_to_cid(code),
        )
    }
}
