use crate::{error::ParserError, parser::PdfParser};

impl PdfParser<'_> {
    /// Parses a PDF null object from the current position in the input stream.
    ///
    /// # Returns
    ///
    /// Returns an error if the `null` keyword is missing or not followed by a delimiter.
    pub fn parse_null_object(&mut self) -> Result<(), ParserError> {
        const NULL_LITERAL: &[u8] = b"null";

        self.read_keyword(NULL_LITERAL)
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
