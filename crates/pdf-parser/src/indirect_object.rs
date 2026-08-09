use pdf_object::{
    indirect_object::IndirectObject, object_resolver::ObjectResolver,
    object_variant::ObjectVariant, stream::StreamObject,
};
use pdf_tokenizer::PdfToken;

use crate::{error::ParserError, parser::PdfParser};

const OBJ_KEYWORD: &[u8] = b"obj";
const ENDOBJ_KEYWORD: &[u8] = b"endobj";

fn starts_with_boundary_keyword(input: &[u8], keyword: &[u8]) -> bool {
    input.starts_with(keyword)
        && match input.get(keyword.len()).copied() {
            Some(next) => PdfParser::is_pdf_delimiter(next),
            None => true,
        }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct IndirectObjectHeader {
    object_number: usize,
    generation_number: usize,
}

/// The two valid continuations after an object and generation number pair.
///
/// PDF syntax uses the same numeric prefix for indirect object declarations and
/// indirect references. Keeping the alternatives explicit prevents callers from
/// treating a reference as an object declaration.
enum IndirectObjectSyntax {
    Object(IndirectObjectHeader),
    Reference { object_number: usize },
}

impl PdfParser<'_> {
    /// Parses an indirect object declaration at `offset` without changing this parser's cursor.
    pub(crate) fn parse_indirect_object_header_at(
        &self,
        offset: usize,
    ) -> Option<IndirectObjectHeader> {
        self.tokenizer.input.get(offset..)?;

        let mut probe = PdfParser::from(self.tokenizer.input);
        probe.tokenizer.position = offset;
        probe.parse_indirect_object_header()
    }

    /// Returns whether an indirect object declaration with the requested identifier starts at `offset`.
    pub(crate) fn matches_indirect_object_header_at(
        &self,
        offset: usize,
        object_number: usize,
        generation_number: usize,
    ) -> bool {
        self.parse_indirect_object_header_at(offset)
            .is_some_and(|header| {
                header.object_number == object_number
                    && header.generation_number == generation_number
            })
    }

    /// Returns whether an indirect object declaration, but not a reference, starts at `offset`.
    pub(crate) fn looks_like_indirect_object_header_at(&self, offset: usize) -> bool {
        self.parse_indirect_object_header_at(offset).is_some()
    }

    /// Returns whether the current input can terminate an object with an omitted `endobj`.
    fn is_at_implicit_endobj_boundary(&self) -> bool {
        let data = self.tokenizer.data();
        if data.is_empty() {
            return true;
        }

        if starts_with_boundary_keyword(data, b"xref")
            || starts_with_boundary_keyword(data, b"trailer")
            || starts_with_boundary_keyword(data, b"startxref")
            || data.starts_with(b"%%EOF")
        {
            return true;
        }

        let mut probe = PdfParser::from(data);
        let Some(_) = probe.read_number::<usize>(true).ok() else {
            return false;
        };
        let Some(_) = probe.read_number::<usize>(true).ok() else {
            return false;
        };

        probe.read_keyword(b"obj").is_ok()
    }

    /// Consumes a standalone `R` reference marker, returning whether one was present.
    fn consume_reference_marker(&mut self) -> bool {
        let data = self.tokenizer.data();
        let is_reference = data.first().copied() == Some(b'R')
            && data.get(1).copied().is_none_or(Self::is_pdf_delimiter);

        if is_reference {
            let _ = self.tokenizer.read();
        }

        is_reference
    }

    /// Parses the shared numeric prefix of an indirect object declaration or reference.
    ///
    /// On failure, the cursor is restored to its initial position so the general object parser
    /// can interpret the same bytes as a number.
    fn parse_indirect_object_syntax(&mut self) -> Option<IndirectObjectSyntax> {
        let mark = self.tokenizer.position;
        let syntax = (|| {
            let object_number = self.read_number(true).ok()?;
            let generation_number = self.read_number(true).ok()?;

            if self.consume_reference_marker() {
                return Some(IndirectObjectSyntax::Reference { object_number });
            }

            self.read_keyword(OBJ_KEYWORD).ok()?;
            Some(IndirectObjectSyntax::Object(IndirectObjectHeader {
                object_number,
                generation_number,
            }))
        })();

        if syntax.is_none() {
            self.tokenizer.position = mark;
        }

        syntax
    }

    /// Parses an indirect object declaration, restoring the cursor when the input is a reference.
    pub(crate) fn parse_indirect_object_header(&mut self) -> Option<IndirectObjectHeader> {
        let mark = self.tokenizer.position;
        let IndirectObjectSyntax::Object(header) = self.parse_indirect_object_syntax()? else {
            self.tokenizer.position = mark;
            return None;
        };

        Some(header)
    }

    /// Requires `endobj`, unless the current input is a safe malformed-file recovery boundary.
    fn consume_endobj_or_implicit_boundary(&mut self) -> Result<(), ParserError> {
        let mark = self.tokenizer.position;
        if self.read_keyword(ENDOBJ_KEYWORD).is_ok() {
            return Ok(());
        }

        self.tokenizer.position = mark;
        if self.is_at_implicit_endobj_boundary() {
            return Ok(());
        }

        self.read_keyword(ENDOBJ_KEYWORD)
    }

    /// Parses the value and terminator for an indirect object declaration.
    fn parse_indirect_object_body(
        &mut self,
        header: IndirectObjectHeader,
        objects: &dyn ObjectResolver,
    ) -> Result<ObjectVariant, ParserError> {
        let object = self.parse_object(objects)?;
        self.skip_whitespace_and_comments();

        if matches!(self.tokenizer.peek(), Some(PdfToken::Alphabetic(b's'))) {
            let ObjectVariant::Dictionary(dictionary) = object else {
                return Err(ParserError::StreamObjectWithoutDictionary);
            };
            let stream = self.parse_stream(&dictionary, objects)?;

            self.skip_whitespace_and_comments();
            self.read_keyword(ENDOBJ_KEYWORD)?;

            return Ok(ObjectVariant::Stream(StreamObject::new_encoded(
                header.object_number,
                header.generation_number,
                dictionary,
                stream,
            )));
        }

        self.consume_endobj_or_implicit_boundary()?;
        Ok(ObjectVariant::IndirectObject(Box::new(
            IndirectObject::new(header.object_number, header.generation_number, Some(object)),
        )))
    }

    /// Parses an indirect object or an object reference from the current position in the input stream.
    pub fn parse_indirect_object(
        &mut self,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<ObjectVariant>, ParserError> {
        let Some(syntax) = self.parse_indirect_object_syntax() else {
            return Ok(None);
        };

        match syntax {
            IndirectObjectSyntax::Object(header) => {
                self.parse_indirect_object_body(header, objects).map(Some)
            }
            IndirectObjectSyntax::Reference { object_number } => {
                Ok(Some(ObjectVariant::Reference(object_number)))
            }
        }
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

    #[test]
    fn test_stream_indirect_object_allows_comment_before_endobj() {
        let input = b"1 0 obj\n<< /Length 5 >>\nstream\nHello\nendstream\n%  %\nendobj\n";
        let mut parser = PdfParser::from(input.as_slice());

        if let Some(ObjectVariant::Stream(stream)) =
            parser.parse_indirect_object(&PassthroughResolver).unwrap()
        {
            assert_eq!(stream.object_number, 1);
            assert_eq!(stream.generation_number, 0);
            assert_eq!(stream.raw_data(), b"Hello");
            assert!(!stream.filters_applied());
        } else {
            panic!("Expected Stream variant");
        }
    }

    #[test]
    fn test_indirect_object_allows_comment_before_endobj() {
        let input = b"1501 0 obj\n61\n% comment\nendobj\n";
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

            assert_eq!(*object_number, 1501);
            assert_eq!(*generation_number, 0);
            assert_eq!(*object, Some(ObjectVariant::Integer(61)));
        } else {
            panic!("Expected IndirectObject variant");
        }
    }

    #[test]
    fn test_indirect_object_allows_missing_endobj_before_next_object() {
        let input = b"1 0 obj\n<< /Type /Catalog >>\n2 0 obj\n<< /Type /Pages >>\nendobj\n";
        let mut parser = PdfParser::from(input.as_slice());

        if let Some(ObjectVariant::IndirectObject(indirect_object)) =
            parser.parse_indirect_object(&PassthroughResolver).unwrap()
        {
            assert_eq!(indirect_object.object_number, 1);
            assert_eq!(
                parser.tokenizer.data(),
                b"2 0 obj\n<< /Type /Pages >>\nendobj\n"
            );
        } else {
            panic!("Expected IndirectObject variant");
        }
    }

    #[test]
    fn test_reference_is_not_treated_as_indirect_object_header() {
        let input = b"1 0 R\n1 0 obj\n";
        let parser = PdfParser::from(input.as_slice());

        assert!(!parser.looks_like_indirect_object_header_at(0));
        assert!(parser.looks_like_indirect_object_header_at(6));
        assert!(parser.matches_indirect_object_header_at(6, 1, 0));
        assert!(!parser.matches_indirect_object_header_at(0, 1, 0));
    }

    #[test]
    fn test_reference_header_parse_restores_cursor() {
        let mut parser = PdfParser::from(b"1 0 R".as_slice());

        assert!(parser.parse_indirect_object_header().is_none());
        assert_eq!(parser.tokenizer.position, 0);
    }

    #[test]
    fn test_indirect_object_header_probe_rejects_out_of_range_offset() {
        let input = b"1 0 obj\n";
        let parser = PdfParser::from(input.as_slice());

        assert!(
            parser
                .parse_indirect_object_header_at(input.len())
                .is_none()
        );
        assert!(
            parser
                .parse_indirect_object_header_at(input.len().saturating_add(1))
                .is_none()
        );
    }

    #[test]
    fn test_non_indirect_syntax_restores_cursor() {
        for input in [
            b"123".as_slice(),
            b"1 0 object".as_slice(),
            b"1 0 Rextra".as_slice(),
            b"1 0 ]".as_slice(),
        ] {
            let mut parser = PdfParser::from(input);

            assert!(
                parser
                    .parse_indirect_object(&PassthroughResolver)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(parser.tokenizer.position, 0);
        }
    }

    #[test]
    fn test_implicit_endobj_requires_keyword_boundary() {
        let input = b"1 0 obj\nnull\nxrefextra";
        let mut parser = PdfParser::from(input.as_slice());

        assert!(parser.parse_indirect_object(&PassthroughResolver).is_err());
    }
}
