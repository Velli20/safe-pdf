use crate::error::FilterError;

/// Converts an ASCII hex digit to its numeric value (0–15).
/// Caller must ensure the byte is a valid hex digit.
fn hex_digit_value(byte: u8) -> Result<u8, FilterError> {
    match byte {
        b'0'..=b'9' => Ok(byte.saturating_sub(b'0')),
        b'a' => Ok(10),
        b'b' => Ok(11),
        b'c' => Ok(12),
        b'd' => Ok(13),
        b'e' => Ok(14),
        b'f' => Ok(15),
        b'A' => Ok(10),
        b'B' => Ok(11),
        b'C' => Ok(12),
        b'D' => Ok(13),
        b'E' => Ok(14),
        b'F' => Ok(15),
        _ => Err(FilterError::Decompression(format!(
            "ASCIIHex: invalid character 0x{byte:02X}"
        ))),
    }
}

/// Decodes ASCIIHexDecode-encoded stream data.
///
/// ASCIIHex encodes binary data as hexadecimal digits. ASCII whitespace is
/// ignored, the `>` character marks end of data, and a final single nibble is
/// treated as the high nibble of the last byte with a low nibble of `0`.
///
/// # Errors
///
/// Returns [`FilterError::Decompression`] if a non-whitespace, non-hexadecimal
/// byte is encountered before the end-of-data marker.
pub(crate) fn decode_ascii_hex(stream_data: &[u8]) -> Result<Vec<u8>, FilterError> {
    let mut output = Vec::with_capacity(stream_data.len() / 2);
    let mut high_nibble: Option<u8> = None;

    for &byte in stream_data {
        if byte == b'>' {
            break;
        }

        if byte.is_ascii_whitespace() {
            continue;
        }

        let nibble = hex_digit_value(byte)?;

        match high_nibble.take() {
            Some(high) => output.push((high << 4) | nibble),
            None => high_nibble = Some(nibble),
        }
    }

    if let Some(high) = high_nibble {
        output.push(high << 4);
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_ascii_hex_basic() {
        let decoded = decode_ascii_hex(b"48656c6c6f>").expect("decode failed");
        assert_eq!(&decoded, b"Hello");
    }

    #[test]
    fn test_decode_ascii_hex_whitespace_ignored() {
        let decoded = decode_ascii_hex(b"48 65\n6c\t6c 6f>").expect("decode failed");
        assert_eq!(&decoded, b"Hello");
    }

    #[test]
    fn test_decode_ascii_hex_odd_nibble_is_padded() {
        let decoded = decode_ascii_hex(b"6>").expect("decode failed");
        assert_eq!(decoded, vec![0x60]);
    }

    #[test]
    fn test_decode_ascii_hex_stops_at_end_marker() {
        let decoded = decode_ascii_hex(b"48656c6c6f>garbage").expect("decode failed");
        assert_eq!(&decoded, b"Hello");
    }

    #[test]
    fn test_decode_ascii_hex_invalid_character_is_error() {
        let result = decode_ascii_hex(b"48GG>");
        assert!(result.is_err());
    }
}
