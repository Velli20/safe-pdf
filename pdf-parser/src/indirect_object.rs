use std::rc::Rc;

use pdf_object::{ObjectVariant, Value, indirect_object::IndirectObject, stream::StreamObject};
use pdf_tokenizer::PdfToken;

use crate::{ParseObject, PdfParser, StreamParser, error::ParserError};

/// Represents an error that can occur while parsing an indirect object or an object reference.
#[derive(Debug, PartialEq)]
pub enum IndirectObjectError {
    /// Indicates that there was an error while parsing the object.
    InvalidObject(String),
    MissingObjectNumber,
    MissingGenerationNumber,
}

impl ParseObject<ObjectVariant> for PdfParser<'_> {
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
    fn parse(&mut self) -> Result<ObjectVariant, ParserError> {
        const OBJ_KEYWORD: &[u8] = b"obj";
        const ENDOBJ_KEYWORD: &[u8] = b"endobj";

        // Read the object number.
        let object_number = self
            .read_number()
            .map_err(|err| IndirectObjectError::MissingObjectNumber)?;

        // Read the generation number.
        let generation_number = self
            .read_number()
            .map_err(|err| IndirectObjectError::MissingGenerationNumber)?;

        // If the next token is 'R', it means this is an object reference.
        if let Some(PdfToken::Alphabetic(b'R')) = self.tokenizer.peek()? {
            self.tokenizer.read();
            return Ok(ObjectVariant::Reference(object_number));
        }

        // Read the keyword `obj`.
        self.read_keyword(OBJ_KEYWORD)?;

        // Parse the object.
        let object = self
            .parse_object()
            .map_err(|e| ParserError::from(IndirectObjectError::InvalidObject(e.to_string())))?;

        self.skip_whitespace();

        if let Some(PdfToken::Alphabetic(b's')) = self.tokenizer.peek()? {
            if let Value::Dictionary(dictionary) = &object {
                let stream = self.parse_stream(dictionary)?;

                // Read the keyword `endobj`.
                self.read_keyword(ENDOBJ_KEYWORD)?;

                return Ok(ObjectVariant::Stream(Rc::new(StreamObject::new(
                    object_number,
                    generation_number,
                    dictionary.clone(),
                    stream,
                ))));
            } else {
                return Err(ParserError::StreamObjectWithoutDictionary);
            }
        }

        // Read the keyword `endobj`.
        self.read_keyword(ENDOBJ_KEYWORD)?;

        return Ok(ObjectVariant::IndirectObject(Rc::new(IndirectObject::new(
            object_number,
            generation_number,
            Some(object),
        ))));
    }
}

impl std::fmt::Display for IndirectObjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndirectObjectError::InvalidObject(e) => {
                write!(f, "Error while parsing indirect object: {}", e)
            }
            IndirectObjectError::MissingObjectNumber => {
                write!(f, "Missing object number")
            }
            IndirectObjectError::MissingGenerationNumber => {
                write!(f, "Missing generation number")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pdf_object::{Value, literal_string::LiteralString};

    use super::*;

    #[test]
    fn test_indirect_object_valid() {
        let input = b"0 1 obj\n(HELLO)\nendobj\n";
        let mut parser = PdfParser::from(input.as_slice());

        if let ObjectVariant::IndirectObject(indirect_object) = parser.parse().unwrap() {
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
                Some(Value::LiteralString(LiteralString::new(String::from(
                    "HELLO"
                ),)))
            );
        } else {
            panic!("Expected IndirectObject variant");
        }
    }
}
