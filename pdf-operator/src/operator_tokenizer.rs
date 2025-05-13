use alloc::borrow::Cow;
use pdf_object::comment::Comment;
use pdf_parser::{ParseObject, PdfParser, error::ParserError};
use pdf_tokenizer::PdfToken;

use crate::error::PdfPainterError;

/// Defines a trait for reading PDF operators and their operands from an input source.
///
/// This trait abstracts the underlying data source and provides methods to read
/// specific types of data required for parsing PDF content stream operators.
pub trait OperatorReader<'a> {
    /// Reads the name of a PDF operator from the input.
    ///
    /// Operator names are typically one or two alphabetic characters.
    /// Whitespace preceding the operator name is skipped.
    fn read_operation_name(&mut self) -> Result<Cow<'a, str>, PdfPainterError>;

    /// Reads a floating-point number from the input.
    fn read_operand(&mut self) -> Result<f32, PdfPainterError>;

    /// Skips whitespaces and comments.
    fn skip_whitespace_and_comments(&mut self) -> Result<(), PdfPainterError>;
}

impl<'a> OperatorReader<'a> for PdfParser<'a> {
    fn read_operation_name(&mut self) -> Result<Cow<'a, str>, PdfPainterError> {
        self.skip_whitespace();

        let name_bytes = self
            .tokenizer
            .read_while_u8(|b| b.is_ascii_alphabetic() || b == b'*');
        if name_bytes.is_empty() {
            return Ok(Cow::Borrowed(""));
        }

        Ok(String::from_utf8_lossy(&name_bytes))
    }

    fn read_operand(&mut self) -> Result<f32, PdfPainterError> {
        self.skip_whitespace();
        let mut has_minus = false;

        // 1. Check for optional sign.
        if let Some(PdfToken::Plus) = self.tokenizer.peek()? {
            self.tokenizer.read();
        } else if let Some(PdfToken::Minus) = self.tokenizer.peek()? {
            self.tokenizer.read();
            has_minus = true;
        }
        // 2. Parse leading digits (integral part).
        let digits = self.read_number::<i32>()?;

        // 3. Check for decimal point
        if let Some(PdfToken::Period) = self.tokenizer.peek()? {
            self.tokenizer.read();
            // 4. Parse fractional part.
            let fraction = self.read_number::<i32>()?;
            // 5. Combine integral and fractional parts.
            let number_str = if has_minus {
                format!("-{}.{}", digits, fraction)
            } else {
                format!("{}.{}", digits, fraction)
            };
            // 6. Convert to f64.
            let number = number_str
                .parse::<f32>()
                .map_err(|err| PdfPainterError::OperandTokenizationError(err.to_string()))?;

            self.skip_whitespace();
            Ok(number)
        } else {
            // 7. No decimal point, parse as integer.
            self.skip_whitespace();
            if has_minus {
                Ok(-(digits as f32))
            } else {
                Ok(digits as f32)
            }
        }
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), PdfPainterError> {
        self.skip_whitespace();

        if let Some(PdfToken::Percent) = self.tokenizer.peek()? {
            let _comment: Result<Comment, ParserError> = self.parse();
        }
        Ok(())
    }
}
