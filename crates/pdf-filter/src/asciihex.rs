/// Converts an ASCII hex digit to its numeric value (0–15), or `None` for any
/// other byte.
fn hex_digit_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte.saturating_sub(b'0')),
        b'a'..=b'f' => Some(byte.saturating_sub(b'a').saturating_add(10)),
        b'A'..=b'F' => Some(byte.saturating_sub(b'A').saturating_add(10)),
        _ => None,
    }
}

/// Decodes ASCIIHexDecode-encoded stream data.
///
/// ASCIIHex encodes binary data as hexadecimal digits. The `>` character marks
/// end of data, and a final single nibble is treated as the high nibble of the
/// last byte with a low nibble of `0`. Every other byte that is not a
/// hexadecimal digit, whitespace or not, is ignored, so malformed data decodes
/// to whatever digits it contains instead of failing.
pub(crate) fn decode_ascii_hex(stream_data: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(stream_data.len() / 2);
    let mut high_nibble: Option<u8> = None;

    for &byte in stream_data {
        if byte == b'>' {
            break;
        }

        let Some(nibble) = hex_digit_value(byte) else {
            continue;
        };

        match high_nibble.take() {
            Some(high) => output.push((high << 4) | nibble),
            None => high_nibble = Some(nibble),
        }
    }

    if let Some(high) = high_nibble {
        output.push(high << 4);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_ascii_hex_basic() {
        let decoded = decode_ascii_hex(b"48656c6c6f>");
        assert_eq!(&decoded, b"Hello");
    }

    #[test]
    fn test_decode_ascii_hex_whitespace_ignored() {
        let decoded = decode_ascii_hex(b"48 65\n6c\t6c 6f>");
        assert_eq!(&decoded, b"Hello");
    }

    #[test]
    fn test_decode_ascii_hex_odd_nibble_is_padded() {
        let decoded = decode_ascii_hex(b"6>");
        assert_eq!(decoded, vec![0x60]);
    }

    #[test]
    fn test_decode_ascii_hex_stops_at_end_marker() {
        let decoded = decode_ascii_hex(b"48656c6c6f>garbage");
        assert_eq!(&decoded, b"Hello");
    }

    #[test]
    fn test_decode_ascii_hex_skips_non_hex_bytes() {
        let decoded = decode_ascii_hex(b"4G8>");
        assert_eq!(decoded, vec![0x48]);
    }
}
