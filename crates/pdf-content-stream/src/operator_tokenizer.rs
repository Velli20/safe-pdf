use alloc::borrow::Cow;
use pdf_parser::{parser::PdfParser, traits::CommentParser};
use pdf_tokenizer::PdfToken;

use crate::error::PdfOperatorError;

/// Defines a trait for reading PDF operators and their operands from an input source.
///
/// This trait abstracts the underlying data source and provides methods to read
/// specific types of data required for parsing PDF content stream operators.
pub trait OperatorReader<'a> {
    /// Reads the name of a PDF operator from the input.
    ///
    /// Operator names are typically one or two alphabetic characters.
    /// Whitespace preceding the operator name is skipped.
    fn read_operation_name(&mut self) -> Result<Cow<'a, str>, PdfOperatorError>;

    /// Skips whitespaces and comments.
    fn skip_whitespace_and_comments(&mut self) -> Result<(), PdfOperatorError>;
}

impl<'a> OperatorReader<'a> for PdfParser<'a> {
    fn read_operation_name(&mut self) -> Result<Cow<'a, str>, PdfOperatorError> {
        self.skip_whitespace();

        // Check for special single-character operators: ' and "
        // These are valid PDF text-showing operators:
        // - ' (single quote): Move to next line and show text
        // - " (double quote): Set word and character spacing, move to next line, show text
        if let Some(&byte) = self.tokenizer.data().first() {
            if byte == b'\'' {
                let _ = self.tokenizer.read_excactly(1);
                return Ok(Cow::Borrowed("'"));
            }
            if byte == b'"' {
                let _ = self.tokenizer.read_excactly(1);
                return Ok(Cow::Borrowed("\""));
            }
        }

        // Read standard operator names:
        // - Alphabetic characters (a-z, A-Z): most operators (q, Q, cm, BT, ET, Tf, etc.)
        // - '*' suffix: path/clipping operators (f*, B*, b*, W*, T*)
        // - '0', '1' suffix: Type 3 font operators (d0, d1)
        let name_bytes = self
            .tokenizer
            .read_while_u8(|b| b.is_ascii_alphabetic() || b == b'*' || b == b'0' || b == b'1');

        if name_bytes.is_empty() {
            return Ok(Cow::Borrowed(""));
        }

        Ok(String::from_utf8_lossy(name_bytes))
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), PdfOperatorError> {
        // Loop to handle multiple consecutive comments and whitespace
        self.skip_whitespace();

        if let Some(PdfToken::Percent) = self.tokenizer.peek() {
            let _comment = self.parse_comment();
        }
        Ok(())
    }
}
