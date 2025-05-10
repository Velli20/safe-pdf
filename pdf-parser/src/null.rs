use pdf_object::null::NullObject;

use crate::{ParseObject, PdfParser, error::ParserError};

impl ParseObject<NullObject> for PdfParser<'_> {
    /// Parses a null object from the current position in the input stream.
    ///
    /// According to PDF 1.7, Section 7.3.7:
    /// The null object is represented by the keyword `null`. It signifies the absence
    /// of a value.
    ///
    /// Like any keyword, 'null' must be followed by a delimiter character or EOF.
    fn parse(&mut self) -> Result<NullObject, ParserError> {
        const NULL_LITERAL: &[u8] = b"null";

        self.read_keyword(NULL_LITERAL)?;

        Ok(NullObject::new())
    }
}

#[cfg(test)]
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
            let result: Result<NullObject, ParserError> = parser.parse();
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
            let result: Result<NullObject, ParserError> = parser.parse();
            assert!(result.is_err());
        }
    }
}
