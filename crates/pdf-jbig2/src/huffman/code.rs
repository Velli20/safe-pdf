use crate::error::Jbig2Error;

/// Maximum Huffman prefix length supported by the JBIG2 standard tables.
///
/// ITU-T T.88 / ISO/IEC 14492 Annex B uses up to 32 extra bits for open-ended
/// integer ranges. The canonical prefix-code assignment is bounded to the same
/// bit width carried by [`HuffmanCode::code`].
const MAX_CODE_LEN: usize = 32;

/// A canonical Huffman code assigned to one range-table entry.
///
/// The `codelen` and `code` fields correspond to `PREFLEN` and the canonical
/// prefix code assigned by ITU-T T.88 / ISO/IEC 14492 Annex B, "Huffman Table
/// Decoding Procedure".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HuffmanCode {
    /// Number of bits in the canonical prefix code.
    pub(crate) codelen: u8,
    /// Canonical prefix code stored in the low `codelen` bits.
    pub(crate) code: u32,
}

/// Assign canonical Huffman codes for the supplied prefix lengths.
///
/// This follows the canonical-code construction used by ITU-T T.88 /
/// ISO/IEC 14492 Annex B before decoding a standard or symbol-ID Huffman
/// table.
pub(crate) fn assign_canonical_codes(lengths: &[u8]) -> Result<Vec<HuffmanCode>, Jbig2Error> {
    assign_canonical_codes_from_lengths(lengths.len(), lengths.iter().copied())
}

/// Assign canonical Huffman codes from a reusable prefix-length iterator.
///
/// `length_count` is used only to reserve the output code vector. The iterator
/// is cloned so the assignment can count prefix lengths first, then emit codes
/// in the original symbol order without requiring a temporary length buffer.
pub(crate) fn assign_canonical_codes_from_lengths<I>(
    length_count: usize,
    lengths: I,
) -> Result<Vec<HuffmanCode>, Jbig2Error>
where
    I: IntoIterator<Item = u8> + Clone,
{
    let mut counts = [0u32; MAX_CODE_LEN.saturating_add(1)];
    let mut max_len = 0u8;
    for codelen in lengths.clone() {
        if codelen == 0 {
            continue;
        }

        let Some(count) = counts.get_mut(usize::from(codelen)) else {
            return Err(Jbig2Error::InvalidTable("Huffman code length"));
        };
        *count = count
            .checked_add(1)
            .ok_or(Jbig2Error::InvalidTable("Huffman code length"))?;
        if codelen > max_len {
            max_len = codelen;
        }
    }

    let mut first_codes = [0u32; MAX_CODE_LEN.saturating_add(1)];
    let mut code = 0u32;
    for (&previous_count, first_code) in counts
        .iter()
        .take(usize::from(max_len))
        .zip(first_codes.iter_mut().skip(1))
    {
        code = code
            .checked_add(previous_count)
            .and_then(|value| value.checked_shl(1))
            .ok_or(Jbig2Error::InvalidTable("Huffman code length"))?;
        *first_code = code;
    }

    let mut codes = Vec::with_capacity(length_count);
    for codelen in lengths {
        if codelen == 0 {
            codes.push(HuffmanCode { codelen, code: 0 });
            continue;
        }

        let index = usize::from(codelen);
        let Some(next_code) = first_codes.get_mut(index) else {
            return Err(Jbig2Error::InvalidTable("Huffman code length"));
        };
        let code = *next_code;
        *next_code = next_code
            .checked_add(1)
            .ok_or(Jbig2Error::InvalidTable("Huffman code length"))?;
        codes.push(HuffmanCode { codelen, code });
    }

    Ok(codes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assigns_zero_length_entries_without_codes() {
        let codes = assign_canonical_codes(&[0, 1, 0, 2]).expect("codes");
        assert_eq!(
            codes.first(),
            Some(&HuffmanCode {
                codelen: 0,
                code: 0
            })
        );
        assert_eq!(
            codes.get(1),
            Some(&HuffmanCode {
                codelen: 1,
                code: 0
            })
        );
        assert_eq!(
            codes.get(2),
            Some(&HuffmanCode {
                codelen: 0,
                code: 0
            })
        );
        assert_eq!(
            codes.get(3),
            Some(&HuffmanCode {
                codelen: 2,
                code: 2
            })
        );
    }

    #[test]
    fn assigns_multiple_codes_per_length_in_input_order() {
        let codes = assign_canonical_codes(&[3, 3, 2, 3, 2]).expect("codes");

        assert_eq!(
            codes.as_slice(),
            [
                HuffmanCode {
                    codelen: 3,
                    code: 4
                },
                HuffmanCode {
                    codelen: 3,
                    code: 5
                },
                HuffmanCode {
                    codelen: 2,
                    code: 0
                },
                HuffmanCode {
                    codelen: 3,
                    code: 6
                },
                HuffmanCode {
                    codelen: 2,
                    code: 1
                },
            ]
        );
    }

    #[test]
    fn rejects_lengths_larger_than_supported_code_width() {
        let err = assign_canonical_codes(&[33]).expect_err("invalid length");

        assert_eq!(err, Jbig2Error::InvalidTable("Huffman code length"));
    }
}
