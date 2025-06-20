use std::rc::Rc;

use pdf_object::{ObjectVariant, indirect_object::IndirectObject, stream::StreamObject};
use pdf_tokenizer::{PdfToken, error::TokenizerError};
use thiserror::Error;

use crate::{
    PdfParser,
    error::ParserError,
    stream::StreamParsingError,
    traits::{IndirectObjectParser, StreamParser},
};

/// Represents an error that can occur while parsing an indirect object or an object reference.
#[derive(Error, Debug, PartialEq)]
pub enum IndirectObjectError {
    /// Indicates that there was an error while parsing the object within the indirect object.
    #[error("Error while parsing object within indirect object: {source}")]
    InvalidObject {
        #[source]
        source: ParserError,
    },
    /// Indicates that the object number is missing.
    #[error("Missing object number in indirect object: {source}")]
    MissingObjectNumber {
        #[source]
        source: ParserError,
    },
    /// Indicates that the generation number is missing.
    #[error("Missing generation number in indirect object: {source}")]
    MissingGenerationNumber {
        #[source]
        source: ParserError,
    },
    /// Indicates that a stream object was encountered without a preceding dictionary.
    #[error("Stream object found without a preceding dictionary")]
    StreamObjectWithoutDictionary,
    /// Propagates errors from stream parsing.
    #[error("Stream parsing error: {0}")]
    StreamError(#[from] StreamParsingError),
    /// Indicates an error while parsing the 'obj' keyword.
    #[error("Failed to parse 'obj' keyword: {source}")]
    InvalidObjKeyword {
        #[source]
        source: ParserError,
    },
    /// Indicates an error while parsing the 'endobj' keyword.
    #[error("Failed to parse 'endobj' keyword: {source}")]
    InvalidEndObjKeyword {
        #[source]
        source: ParserError,
    },
    #[error("Tokenizer error: {0}")]
    TokenizerError(#[from] TokenizerError),
    #[error("Parser error: {0}")]
    ParserError(#[from] ParserError),
}

impl IndirectObjectParser for PdfParser<'_> {
    type ErrorType = IndirectObjectError;

    /// Parses an indirect object or an object reference from the current position in the input stream.
    ///
    /// # Indirect Object
    ///
    /// According to the PDF 1.7 Specification, Section 7.3.10 Indirect Objects:
    /// - An indirect object reference consists of an object number, a generation number,
    ///   and the keyword `obj`.
    /// - The object number and generation number are separated by a space.
    /// - Ends with the keyword `endobj`.
    ///
    /// ## Example input
    ///
    /// ```text
    /// 15 0 obj
    /// << /Type /Catalog /Pages 1 0 R >>
    /// endobj
    /// ```
    ///
    /// # Object Reference
    ///
    /// An object reference in a PDF is an indirect reference to another object, and has the form:
    /// ```text
    /// <object-number> <generation-number> R
    /// ```
    ///
    /// - `<object-number>`: A positive integer (object number) identifying the indirect object.
    /// - `<generation-number>`: A non-negative integer representing the generation of the object.
    /// - `R`: A literal keyword that must follow the two integers and must be separated by whitespace.
    ///
    /// ## Example input
    ///
    /// ```text
    /// 15 0 R
    /// ```
    fn parse_indirect_object(&mut self) -> Result<Option<ObjectVariant>, Self::ErrorType> {
        const OBJ_KEYWORD: &[u8] = b"obj";
        const ENDOBJ_KEYWORD: &[u8] = b"endobj";

        // Read the object number.
        let Some(object_number) = self.read_number(true).ok() else {
            return Ok(None);
        };

        // Read the generation number.
        let Some(generation_number) = self.read_number(true).ok() else {
            return Ok(None);
        };

        // If the next token is 'R', it means this is an object reference.
        if let Some(PdfToken::Alphabetic(b'R')) = self.tokenizer.peek() {
            if let Some(s) = self.tokenizer.data().get(1) {
                if Self::is_pdf_delimiter(*s) {
                    self.tokenizer.read();
                    return Ok(Some(ObjectVariant::Reference(object_number)));
                }
            } else {
                self.tokenizer.read();
                return Ok(Some(ObjectVariant::Reference(object_number)));
            }
        }

        // Read the keyword `obj`.
        let Some(()) = self.read_keyword(OBJ_KEYWORD).ok() else {
            return Ok(None);
        };

        // Parse the object.
        let object = self
            .parse_object()
            .map_err(|source| IndirectObjectError::InvalidObject { source })?;

        self.skip_whitespace();

        if let Some(PdfToken::Alphabetic(b's')) = self.tokenizer.peek() {
            if let ObjectVariant::Dictionary(dictionary) = &object {
                let stream = self.parse_stream(dictionary)?;

                // Read the keyword `endobj`.
                self.read_keyword(ENDOBJ_KEYWORD)
                    .map_err(|source| IndirectObjectError::InvalidEndObjKeyword { source })?;

                return Ok(Some(ObjectVariant::Stream(Rc::new(StreamObject::new(
                    object_number,
                    generation_number,
                    dictionary.clone(),
                    stream,
                )))));
            } else {
                return Err(IndirectObjectError::StreamObjectWithoutDictionary);
            }
        }

        // Read the keyword `endobj`.
        self.read_keyword(ENDOBJ_KEYWORD)
            .map_err(|source| IndirectObjectError::InvalidEndObjKeyword { source })?;

        return Ok(Some(ObjectVariant::IndirectObject(Rc::new(
            IndirectObject::new(object_number, generation_number, Some(object)),
        ))));
    }
}

#[cfg(test)]
mod tests {
    use pdf_object::ObjectVariant;

    use super::*;

    #[test]
    fn test_indirect_object_valid() {
        let input = b"0 1 obj\n(HELLO)\nendobj\n";
        let mut parser = PdfParser::from(input.as_slice());

        if let Some(ObjectVariant::IndirectObject(indirect_object)) =
            parser.parse_indirect_object().unwrap()
        {
            let IndirectObject {
                object_number,
                generation_number,
                object,
                ..
            } = indirect_object.as_ref();

            assert_eq!(*object_number, 0);
            assert_eq!(*generation_number, 1);
            assert_eq!(
                *object,
                Some(ObjectVariant::LiteralString(String::from("HELLO"),))
            );
        } else {
            panic!("Expected IndirectObject variant");
        }
    }
}
