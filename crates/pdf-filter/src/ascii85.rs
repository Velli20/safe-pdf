use crate::error::FilterError;

/// Decodes ASCII85Decode-encoded stream data.
///
/// ASCII85 encodes 4 binary bytes as 5 printable ASCII characters in the
/// range `!` (33) through `u` (117), using base 85. The special symbol `z`
/// represents a group of four zero bytes. Whitespace is ignored. The
/// end-of-data marker `~>` terminates the stream; any bytes after it are
/// discarded.
///
/// # Errors
///
/// Returns [`FilterError::Decompression`] if an invalid character is
/// encountered (outside the expected ASCII85 alphabet).
pub(crate) fn decode_ascii85(stream_data: &[u8]) -> Result<Vec<u8>, FilterError> {
    let mut output = Vec::with_capacity(stream_data.len().saturating_div(5).saturating_mul(4));
    let mut group = [0u32; 5];
    let mut group_len = 0usize;

    for &byte in stream_data {
        // End-of-data marker `~>`
        if byte == b'~' {
            break;
        }

        // Skip whitespace
        if byte.is_ascii_whitespace() {
            continue;
        }

        // `z` shorthand: four zero bytes (only valid at a group boundary)
        if byte == b'z' {
            if group_len != 0 {
                return Err(FilterError::Decompression(
                    "ASCII85: 'z' encountered in the middle of a group".to_string(),
                ));
            }
            output.extend_from_slice(&[0u8; 4]);
            continue;
        }

        if !(b'!'..=b'u').contains(&byte) {
            return Err(FilterError::Decompression(format!(
                "ASCII85: invalid character 0x{byte:02X}"
            )));
        }

        if let Some(slot) = group.get_mut(group_len) {
            // byte is validated to be in b'!'..=b'u', so wrapping_sub is safe
            *slot = u32::from(byte.wrapping_sub(b'!'));
        }
        group_len = group_len.saturating_add(1);

        if group_len == 5 {
            // Horner's method: avoids large intermediate exponents
            let val = group
                .iter()
                .fold(0u32, |acc, &d| acc.wrapping_mul(85).wrapping_add(d));
            output.extend_from_slice(&val.to_be_bytes());
            group_len = 0;
        }
    }

    // Handle the final partial group (1–4 chars → 1–3 bytes)
    if group_len > 0 {
        let partial_bytes = group_len.saturating_sub(1);
        // Pad remaining slots with `u` (value 84) per PDF spec §7.4.3
        for slot in group.iter_mut().skip(group_len) {
            *slot = 84;
        }
        let val = group
            .iter()
            .fold(0u32, |acc, &d| acc.wrapping_mul(85).wrapping_add(d));
        let bytes = val.to_be_bytes();
        if let Some(slice) = bytes.get(..partial_bytes) {
            output.extend_from_slice(slice);
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_ascii85_basic() {
        // "Man " → known ASCII85 encoding "9jqo^"
        let man_encoded = b"9jqo^~>";
        let decoded = decode_ascii85(man_encoded).expect("decode failed");
        assert_eq!(decoded, b"Man ");
    }

    #[test]
    fn test_decode_ascii85_z_shorthand() {
        // `z` represents 4 zero bytes
        let input = b"z~>";
        let decoded = decode_ascii85(input).expect("decode failed");
        assert_eq!(decoded, [0u8; 4]);
    }

    #[test]
    fn test_decode_ascii85_multiple_groups() {
        // "Man is di" — two full groups + one partial group (1 byte → 2 chars)
        let input = b"9jqo^BlbD-B`~>";
        let decoded = decode_ascii85(input).expect("decode failed");
        assert_eq!(&decoded, b"Man is di");
    }

    #[test]
    fn test_decode_ascii85_whitespace_ignored() {
        // Whitespace (spaces, newlines) must be ignored
        let input = b"9j qo\n^~>";
        let decoded = decode_ascii85(input).expect("decode failed");
        assert_eq!(&decoded, b"Man ");
    }

    #[test]
    fn test_decode_ascii85_partial_group() {
        // 2 input chars → 1 output byte
        // Encode 0xAB: pad to [0xAB, 84<<24…] etc.
        // 0xAB000000 in base85: 0xAB000000 = 2,869,231,616
        // / 85^4 = 2869231616 / 52200625 = 54 → char '!'+ 54 = 'W'
        // remainder: 2869231616 - 54*52200625 = 2869231616 - 2818833750 = 50397866
        // / 85^3 = 50397866 / 614125 = 82 → char '!'+ 82 = 's'
        // So first two chars of the full 5-char group for 0xAB000000 are "Ws..."
        // We only need the first 2 chars to recover 1 byte.
        let input = b"Ws~>";
        let decoded = decode_ascii85(input).expect("decode failed");
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0], 0xAB);
    }

    #[test]
    fn test_decode_ascii85_invalid_char() {
        let input = b"9jqo\x80~>";
        let result = decode_ascii85(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_ascii85_z_in_middle_of_group_is_error() {
        // 'z' mid-group is invalid
        let input = b"9jz~>";
        let result = decode_ascii85(input);
        assert!(result.is_err());
    }
}
