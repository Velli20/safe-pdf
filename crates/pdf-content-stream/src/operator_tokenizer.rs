use pdf_parser::parser::PdfParser;

use crate::error::PdfOperatorError;

/// Reads a PDF operator name from `parser`, advancing its cursor past the name.
///
/// Must be called when the parser is positioned at the first byte of an operator
/// name — i.e. `'`, `"`, or an ASCII alphabetic byte. Whitespace must already
/// have been consumed by the caller.
///
/// For the two single-character text operators (`'` and `"`) this returns a
/// `&'static str` literal coerced to `&'a str`. For all other operator names
/// (alphabetic characters optionally followed by `*`, `0`, or `1`) it returns a
/// zero-copy slice of the parser's input buffer.
///
/// # Errors
///
/// Returns [`PdfOperatorError::UnknownOperator`] when no valid operator-name
/// bytes are found at the current position (e.g. input is exhausted or the
/// current byte is not a recognised operator character).
pub(crate) fn read_operator_name<'a>(
    parser: &mut PdfParser<'a>,
) -> Result<&'a str, PdfOperatorError> {
    let first = parser.tokenizer.data().first().copied();
    match first {
        Some(b'\'') => {
            let _ = parser.tokenizer.read_excactly(1)?;
            Ok("'")
        }
        Some(b'"') => {
            let _ = parser.tokenizer.read_excactly(1)?;
            Ok("\"")
        }
        _ => {
            // Standard operator names: ASCII letters optionally suffixed with
            // `*` (f*, B*, b*, W*, T*) or `0`/`1` (d0, d1 — Type 3 font ops).
            let name_bytes = parser
                .tokenizer
                .read_while_u8(|b| b.is_ascii_alphabetic() || b == b'*' || b == b'0' || b == b'1');

            if name_bytes.is_empty() {
                return Err(PdfOperatorError::UnknownOperator(first.map_or_else(
                    || "(end of input)".to_string(),
                    |b| format!("0x{b:02X}"),
                )));
            }

            // The predicate above only matches ASCII bytes — a strict subset of
            // valid UTF-8 — so `from_utf8` is guaranteed to succeed here.
            std::str::from_utf8(name_bytes).map_err(|_| {
                PdfOperatorError::UnknownOperator(String::from_utf8_lossy(name_bytes).into_owned())
            })
        }
    }
}
