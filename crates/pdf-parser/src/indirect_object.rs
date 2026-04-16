use pdf_object::{
    indirect_object::IndirectObject, object_resolver::ObjectResolver,
    object_variant::ObjectVariant, stream::StreamObject,
};
use pdf_tokenizer::PdfToken;

use crate::{error::ParserError, parser::PdfParser};

impl PdfParser<'_> {
    /// Parses an indirect object or an object reference from the current position in the input stream.
    pub fn parse_indirect_object(
        &mut self,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<ObjectVariant>, ParserError> {
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
        if self.read_keyword(OBJ_KEYWORD).is_err() {
            return Ok(None);
        };

        // Parse the object.
        let object = self.parse_object(objects)?;

        self.skip_whitespace();

        if let Some(PdfToken::Alphabetic(b's')) = self.tokenizer.peek() {
            let ObjectVariant::Dictionary(dictionary) = object else {
                return Err(ParserError::StreamObjectWithoutDictionary);
            };
            let stream = self.parse_stream(&dictionary, objects)?;

            // Read the keyword `endobj`.
            self.read_keyword(ENDOBJ_KEYWORD)?;

            return Ok(Some(ObjectVariant::Stream(StreamObject::new(
                object_number,
                generation_number,
                dictionary,
                stream,
            ))));
        }

        // Read the keyword `endobj`.
        self.read_keyword(ENDOBJ_KEYWORD)?;

        Ok(Some(ObjectVariant::IndirectObject(Box::new(
            IndirectObject::new(object_number, generation_number, Some(object)),
        ))))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use pdf_object::{object_resolver::PassthroughResolver, object_variant::ObjectVariant};

    use super::*;

    #[test]
    fn test_indirect_object_valid() {
        let input = b"0 1 obj\n(HELLO)\nendobj\n";
        let mut parser = PdfParser::from(input.as_slice());
        if let Some(ObjectVariant::IndirectObject(indirect_object)) =
            parser.parse_indirect_object(&PassthroughResolver).unwrap()
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
                Some(ObjectVariant::LiteralString(b"HELLO".to_vec()))
            );
        } else {
            panic!("Expected IndirectObject variant");
        }
    }
}
