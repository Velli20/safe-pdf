use crate::error::FilterError;

/// Decodes RunLengthDecode-compressed stream data.
///
/// PDF RunLength data is organized into packets prefixed by a single header
/// byte. Literal packets use header values `0..=127` and copy the next
/// `header + 1` bytes verbatim. Repeat packets use header values `129..=255`
/// and repeat the following byte `257 - header` times. Header value `128`
/// terminates the stream.
///
/// # Errors
///
/// Returns [`FilterError::Decompression`] when a packet is truncated and the
/// decoder cannot read the bytes required by its header.
pub(crate) fn decode_run_length(stream_data: &[u8]) -> Result<Vec<u8>, FilterError> {
    let mut output = Vec::new();
    let mut index = 0usize;

    while let Some(&header) = stream_data.get(index) {
        index = index.saturating_add(1);

        match header {
            128 => break,
            0..=127 => {
                let literal_len = usize::from(header).saturating_add(1);
                let end = index.saturating_add(literal_len);
                let Some(bytes) = stream_data.get(index..end) else {
                    return Err(FilterError::Decompression(
                        "RunLengthDecode: truncated literal run".to_string(),
                    ));
                };
                output.extend_from_slice(bytes);
                index = end;
            }
            129..=255 => {
                let repeat_len = 257usize.saturating_sub(usize::from(header));
                let Some(&byte) = stream_data.get(index) else {
                    return Err(FilterError::Decompression(
                        "RunLengthDecode: truncated repeat run".to_string(),
                    ));
                };
                output.extend(std::iter::repeat_n(byte, repeat_len));
                index = index.saturating_add(1);
            }
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_run_length_literal_run() {
        let decoded = decode_run_length(&[2, b'A', b'B', b'C', 128]).expect("decode failed");
        assert_eq!(decoded, b"ABC");
    }

    #[test]
    fn test_decode_run_length_repeat_run() {
        let decoded = decode_run_length(&[254, b'Z', 128]).expect("decode failed");
        assert_eq!(decoded, b"ZZZ");
    }

    #[test]
    fn test_decode_run_length_mixed_runs() {
        let decoded = decode_run_length(&[2, b'A', b'B', b'C', 255, b'!', 0, b'?', 128])
            .expect("decode failed");
        assert_eq!(decoded, b"ABC!!?");
    }

    #[test]
    fn test_decode_run_length_stops_at_eod() {
        let decoded = decode_run_length(&[0, b'A', 128, 0, b'B']).expect("decode failed");
        assert_eq!(decoded, b"A");
    }

    #[test]
    fn test_decode_run_length_truncated_literal_run_is_error() {
        let result = decode_run_length(&[2, b'A', b'B']);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_run_length_truncated_repeat_run_is_error() {
        let result = decode_run_length(&[255]);
        assert!(result.is_err());
    }
}
