use pdf_object::{
    object_id::PdfObjectId, object_resolver::ObjectResolver, object_variant::ObjectVariant,
    stream::StreamObject,
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

impl PdfParser<'_> {
    /// Parses an indirect object declaration at `offset` without changing this parser's cursor.
    pub(crate) fn parse_indirect_object_id_at(&self, offset: usize) -> Option<PdfObjectId> {
        let mut probe = self.at_offset(offset).ok()?;
        probe.parse_indirect_object_id()
    }

    /// Returns whether an indirect object declaration with the requested identifier starts at `offset`.
    pub(crate) fn matches_indirect_object_header_at(
        &self,
        offset: usize,
        object_number: usize,
        generation_number: usize,
    ) -> bool {
        self.parse_indirect_object_id_at(offset)
            .is_some_and(|identifier| {
                identifier.number == object_number && identifier.generation == generation_number
            })
    }

    /// Returns whether an indirect object declaration, but not a reference, starts at `offset`.
    pub(crate) fn looks_like_indirect_object_header_at(&self, offset: usize) -> bool {
        self.parse_indirect_object_id_at(offset).is_some()
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

    /// Parses a number, recognizing an indirect reference when present.
    pub(crate) fn parse_number_or_reference(&mut self) -> Result<ObjectVariant, ParserError> {
        let mark = self.tokenizer.position;
        let reference = (|| {
            let object_number = self.read_number(true).ok()?;
            let _generation_number = self.read_number::<usize>(true).ok()?;
            self.consume_reference_marker().then_some(object_number)
        })();

        if let Some(object_number) = reference {
            return Ok(ObjectVariant::Reference(object_number));
        }

        self.tokenizer.position = mark;
        self.parse_number()
    }

    /// Parses an indirect object identifier and consumes its declaration header.
    ///
    /// The cursor is restored when the input is not an indirect object declaration.
    /// The object value and terminator are left for the caller.
    pub fn parse_indirect_object_id(&mut self) -> Option<PdfObjectId> {
        let mark = self.tokenizer.position;
        let identifier = (|| {
            let number = self.read_number(true).ok()?;
            let generation = self.read_number(true).ok()?;
            self.read_keyword(OBJ_KEYWORD).ok()?;
            Some(PdfObjectId { number, generation })
        })();

        if identifier.is_none() {
            self.tokenizer.position = mark;
        }

        identifier
    }

    /// Parses the value and terminator following an indirect object identifier.
    ///
    /// The identifier must have already been consumed with
    /// [`Self::parse_indirect_object_id`]. Stream dictionaries are combined with
    /// their encoded bytes and the identifier without wrapping the result.
    pub fn parse_indirect_object_value(
        &mut self,
        identifier: PdfObjectId,
        objects: &dyn ObjectResolver,
    ) -> Result<ObjectVariant, ParserError> {
        let object = self.parse_object(objects)?;
        self.skip_whitespace_and_comments();

        if self.is_at_stream_start() {
            let ObjectVariant::Dictionary(dictionary) = object else {
                return Err(ParserError::StreamObjectWithoutDictionary);
            };
            let data = self.parse_stream(&dictionary, objects)?;
            self.consume_required_endobj()?;
            return Ok(ObjectVariant::Stream(StreamObject::new_encoded(
                identifier.number,
                identifier.generation,
                dictionary,
                data,
            )));
        }

        self.consume_endobj_or_implicit_boundary()?;
        Ok(object)
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

    /// Consumes the required `endobj` keyword without malformed-file recovery.
    fn consume_required_endobj(&mut self) -> Result<(), ParserError> {
        self.skip_whitespace_and_comments();
        self.read_keyword(ENDOBJ_KEYWORD)
    }

    /// Returns whether the current position begins a stream suffix.
    fn is_at_stream_start(&mut self) -> bool {
        matches!(self.tokenizer.peek(), Some(PdfToken::Alphabetic(b's')))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use pdf_object::{object_resolver::PassthroughResolver, object_variant::ObjectVariant};

    use super::*;

    fn parse_staged_indirect_object(
        parser: &mut PdfParser<'_>,
    ) -> Result<Option<(PdfObjectId, ObjectVariant)>, ParserError> {
        let Some(identifier) = parser.parse_indirect_object_id() else {
            return Ok(None);
        };
        let object = parser.parse_indirect_object_value(identifier, &PassthroughResolver)?;
        Ok(Some((identifier, object)))
    }

    #[test]
    fn test_indirect_object_valid() {
        let input = b"0 1 obj\n(HELLO)\nendobj\n";
        let mut parser = PdfParser::from(input.as_slice());
        if let Some((identifier, object)) = parse_staged_indirect_object(&mut parser).unwrap() {
            assert_eq!(identifier.number, 0);
            assert_eq!(identifier.generation, 1);
            assert_eq!(object, ObjectVariant::LiteralString(b"HELLO".to_vec()));
        } else {
            panic!("Expected indirect object declaration");
        }
    }

    #[test]
    fn test_indirect_object_id_leaves_value_for_direct_parser() {
        let mut parser = PdfParser::from(b"12 3 obj\n42\nendobj".as_slice());

        let identifier = parser
            .parse_indirect_object_id()
            .expect("indirect object header should parse");

        assert_eq!(
            identifier,
            PdfObjectId {
                number: 12,
                generation: 3,
            }
        );
        assert_eq!(
            parser.parse_object(&PassthroughResolver).unwrap(),
            ObjectVariant::Integer(42)
        );
    }

    #[test]
    fn test_direct_parser_only_consumes_number_from_indirect_object_declaration() {
        let mut parser = PdfParser::from(b"12 3 obj\n42\nendobj".as_slice());

        assert_eq!(
            parser.parse_object(&PassthroughResolver).unwrap(),
            ObjectVariant::Integer(12)
        );
        assert_eq!(parser.tokenizer.data(), b"3 obj\n42\nendobj");
    }

    #[test]
    fn test_stream_indirect_object_allows_comment_before_endobj() {
        let input = b"1 0 obj\n<< /Length 5 >>\nstream\nHello\nendstream\n%  %\nendobj\n";
        let mut parser = PdfParser::from(input.as_slice());

        if let Some((identifier, ObjectVariant::Stream(stream))) =
            parse_staged_indirect_object(&mut parser).unwrap()
        {
            assert_eq!(identifier.number, 1);
            assert_eq!(identifier.generation, 0);
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

        if let Some((identifier, object)) = parse_staged_indirect_object(&mut parser).unwrap() {
            assert_eq!(identifier.number, 1501);
            assert_eq!(identifier.generation, 0);
            assert_eq!(object, ObjectVariant::Integer(61));
        } else {
            panic!("Expected indirect object declaration");
        }
    }

    #[test]
    fn test_indirect_object_allows_missing_endobj_before_next_object() {
        let input = b"1 0 obj\n<< /Type /Catalog >>\n2 0 obj\n<< /Type /Pages >>\nendobj\n";
        let mut parser = PdfParser::from(input.as_slice());

        if let Some((identifier, _)) = parse_staged_indirect_object(&mut parser).unwrap() {
            assert_eq!(identifier.number, 1);
            assert_eq!(
                parser.tokenizer.data(),
                b"2 0 obj\n<< /Type /Pages >>\nendobj\n"
            );
        } else {
            panic!("Expected indirect object declaration");
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

        assert!(parser.parse_indirect_object_id().is_none());
        assert_eq!(parser.tokenizer.position, 0);
    }

    #[test]
    fn test_indirect_object_header_probe_rejects_out_of_range_offset() {
        let input = b"1 0 obj\n";
        let parser = PdfParser::from(input.as_slice());

        assert!(parser.parse_indirect_object_id_at(input.len()).is_none());
        assert!(
            parser
                .parse_indirect_object_id_at(input.len().saturating_add(1))
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

            assert!(parser.parse_indirect_object_id().is_none());
            assert_eq!(parser.tokenizer.position, 0);
        }
    }

    #[test]
    fn test_implicit_endobj_requires_keyword_boundary() {
        let input = b"1 0 obj\nnull\nxrefextra";
        let mut parser = PdfParser::from(input.as_slice());

        assert!(parse_staged_indirect_object(&mut parser).is_err());
    }
}
