use crate::{error::ParserError, parser::PdfParser, traits::NullObjectParser};

impl NullObjectParser for PdfParser<'_> {
    type ErrorType = ParserError;

    /// Parses a PDF null object from the current position in the input stream.
    ///
    /// According to the PDF 1.7 Specification (Section 7.3.9 "Null Object"):
    /// The null object is used to represent a non-existent or undefined value.
    ///
    /// # Format
    ///
    /// - Represented by the literal keyword `null`.
    /// - The keyword `null` is case-sensitive.
    /// - It must be followed by a delimiter character (e.g., whitespace, `)`, `]`, `>`, `/`, `%`).
    ///
    /// # Example Inputs
    ///
    /// ```text
    /// null
    /// null%comment
    /// null]
    /// ```
    ///
    /// # Returns
    ///
    /// A `()` if the `null` keyword is successfully parsed,
    /// or a `ParserError` if the keyword is not found or is malformed.
    fn parse_null_object(&mut self) -> Result<(), Self::ErrorType> {
        const NULL_LITERAL: &[u8] = b"null";

        self.read_keyword(NULL_LITERAL)?;

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_null_object() {
        let valid_inputs: Vec<&[u8]> = vec![
            b"null\n",
            b"null\t",
            b"null ",
            b"null<",
            b"null>",
            b"null[",
            b"null]",
            b"null{",
            b"null}",
            b"null(abc)",
        ];

        for input in valid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_null_object();
            assert!(result.is_ok());
        }
        let invalid_inputs: Vec<&[u8]> = vec![
            b"nullabc\n",
            b"null123\n",
            b"nulla",
            b"nullobj\n",
            b"nullobj<",
            b"nullobj>",
            b"nullobj[",
            b"nullobj]",
            b"nullobj{",
            b"nullobj}",
        ];
        for input in invalid_inputs {
            let mut parser = PdfParser::from(input);
            let result = parser.parse_null_object();
            assert!(result.is_err());
        }
    }
}
