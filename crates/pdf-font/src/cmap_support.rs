use pdf_parser::{
    cmap::{CMapParser, CMapToken},
    error::ParserError,
};

/// Read the next CMap token from the shared parser.
///
/// This is a thin wrapper so callers can keep token handling centralized
/// without depending directly on `pdf_parser::cmap::CMapParser` internals.
pub(crate) fn next_cmap_token(
    parser: &mut CMapParser<'_>,
) -> Result<Option<CMapToken>, ParserError> {
    parser.next_token()
}

/// Read the next token as a PDF hex string.
///
/// Returns the decoded bytes when the next token is hexadecimal data and a
/// parser error otherwise.
pub(crate) fn expect_hex_token(
    parser: &mut CMapParser<'_>,
    message: &str,
) -> Result<Vec<u8>, ParserError> {
    match next_cmap_token(parser)? {
        Some(CMapToken::HexString(bytes)) => Ok(bytes),
        Some(_) | None => Err(ParserError::InvalidNumber(message.to_string())),
    }
}

/// Read the next token as a signed integer.
///
/// Returns the parsed value when the next token is an integer and a parser
/// error otherwise.
pub(crate) fn expect_integer_token(
    parser: &mut CMapParser<'_>,
    message: &str,
) -> Result<i64, ParserError> {
    match next_cmap_token(parser)? {
        Some(CMapToken::Integer(value)) => Ok(value),
        Some(_) | None => Err(ParserError::InvalidNumber(message.to_string())),
    }
}

/// Convert a big-endian byte slice into a `u32`.
///
/// Bytes beyond four are folded in by repeated left-shifts, matching the
/// existing PDF CMap code paths that only care about the low-order result.
pub(crate) fn bytes_to_u32(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |value, byte| {
        value.checked_shl(8).unwrap_or(0) | u32::from(*byte)
    })
}

/// Advance the parser until the requested operator is found or input ends.
///
/// Returns `true` if the operator was encountered and `false` if the parser
/// reached the end of the stream first.
pub(crate) fn consume_until_operator(
    parser: &mut CMapParser<'_>,
    end_operator: &[u8],
) -> Result<bool, ParserError> {
    loop {
        match next_cmap_token(parser)? {
            Some(CMapToken::Operator(operator)) if operator.as_slice() == end_operator => {
                return Ok(true);
            }
            Some(_) => {}
            None => return Ok(false),
        }
    }
}
