use std::ops::Range;

use bytes::Bytes;
use pdf_object_reader::{
    dictionary::Dictionary, object_id::ObjectId, object_variant::ObjectVariant,
    stream::StreamObject,
};
use pdf_tokenizer::PdfToken;

use crate::{error::ParserError, parser::PdfParser};

const OBJ_KEYWORD: &[u8] = b"obj";
const ENDOBJ_KEYWORD: &[u8] = b"endobj";

/// The value of an indirect object, up to the body of a stream.
#[derive(Debug)]
pub enum IndirectValue {
    /// A complete value whose terminator has been consumed.
    Object(ObjectVariant),
    /// A stream whose body has not been read yet.
    Stream(StreamHeader),
}

/// The dictionary of a stream whose body has not been read.
///
/// Only [`PdfParser::parse_indirect_object_value`] creates one, leaving its parser at the
/// `stream` keyword so that the same parser can read the body.
#[derive(Debug)]
pub struct StreamHeader {
    identifier: ObjectId,
    dictionary: Dictionary,
}

impl StreamHeader {
    /// Returns the stream dictionary.
    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    /// Returns the raw `/Length` entry, which may be a reference to another object.
    pub fn length(&self) -> Option<&ObjectVariant> {
        self.dictionary.get(b"Length")
    }

    /// Combines the dictionary with the body found at `body` in `source`.
    ///
    /// `source` must be the input the parser read, and `body` the range returned by
    /// [`PdfParser::parse_stream_body`] or [`PdfParser::recover_stream_body`]. The stream
    /// shares `source`'s allocation instead of copying the body.
    ///
    /// # Errors
    ///
    /// Returns [`ParserError::UnexpectedEndOfFile`] if `body` lies outside `source`.
    pub fn into_stream_in(
        self,
        source: &Bytes,
        body: Range<usize>,
    ) -> Result<ObjectVariant, ParserError> {
        source
            .get(body.clone())
            .ok_or(ParserError::UnexpectedEndOfFile)?;
        Ok(self.into_stream(source.slice(body)))
    }

    /// Combines the dictionary with its encoded body.
    pub(crate) fn into_stream(self, data: impl Into<Bytes>) -> ObjectVariant {
        ObjectVariant::Stream(StreamObject::new_encoded(
            self.identifier.number,
            self.identifier.generation,
            self.dictionary,
            data,
        ))
    }
}

fn starts_with_boundary_keyword(input: &[u8], keyword: &[u8]) -> bool {
    input.starts_with(keyword)
        && match input.get(keyword.len()).copied() {
            Some(next) => PdfParser::is_pdf_delimiter(next),
            None => true,
        }
}

impl PdfParser<'_> {
    /// Parses an indirect object declaration at `offset` without changing this parser's cursor.
    ///
    /// Offsets inside a number are rejected without reading the digit run, which keeps
    /// byte-by-byte scans over long digit sequences linear.
    pub(crate) fn parse_indirect_object_id_at(&self, offset: usize) -> Option<ObjectId> {
        if offset
            .checked_sub(1)
            .and_then(|index| self.tokenizer.input.get(index))
            .is_some_and(u8::is_ascii_digit)
        {
            return None;
        }

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
            let generation_number = self.read_number::<usize>(true).ok()?;
            self.consume_reference_marker()
                .then_some(ObjectId::new(object_number, generation_number))
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
    pub fn parse_indirect_object_id(&mut self) -> Option<ObjectId> {
        let mark = self.tokenizer.position;
        let identifier = (|| {
            let number = self.read_number(true).ok()?;
            let generation = self.read_number(true).ok()?;
            self.read_keyword(OBJ_KEYWORD).ok()?;
            Some(ObjectId { number, generation })
        })();

        if identifier.is_none() {
            self.tokenizer.position = mark;
        }

        identifier
    }

    /// Parses the value following an indirect object identifier.
    ///
    /// The identifier must have already been consumed with
    /// [`Self::parse_indirect_object_id`]. A plain value is returned with its terminator
    /// consumed. A stream stops at its `stream` keyword: its body length can depend on
    /// other objects, so the caller determines it from the returned [`StreamHeader`],
    /// locates the body with [`Self::parse_stream_body`] or [`Self::recover_stream_body`],
    /// and builds the stream with [`StreamHeader::into_stream_in`].
    pub fn parse_indirect_object_value(
        &mut self,
        identifier: ObjectId,
    ) -> Result<IndirectValue, ParserError> {
        let object = self.parse_object()?;
        self.skip_whitespace_and_comments();

        if self.is_at_stream_start() {
            let ObjectVariant::Dictionary(dictionary) = object else {
                return Err(ParserError::StreamObjectWithoutDictionary);
            };
            return Ok(IndirectValue::Stream(StreamHeader {
                identifier,
                dictionary,
            }));
        }

        self.consume_endobj_or_implicit_boundary()?;
        Ok(IndirectValue::Object(object))
    }

    /// Locates a stream body of exactly `length` bytes, then reads `endstream` and `endobj`.
    ///
    /// The parser must be where [`Self::parse_indirect_object_value`] left it. Returns the
    /// body's byte range within the parser's input; the bytes are neither copied nor
    /// filter decoded.
    ///
    /// # Errors
    ///
    /// Returns an error if the declared bytes are not available, or the `stream`,
    /// `endstream`, or `endobj` keyword is missing or malformed.
    pub fn parse_stream_body(&mut self, length: usize) -> Result<Range<usize>, ParserError> {
        let body = self.parse_stream(length)?;
        self.consume_required_endobj()?;
        Ok(body)
    }

    /// Locates a stream body using `length` only as a hint, then reads `endobj`.
    ///
    /// A declared boundary is used when it is structurally valid; a missing, stale, or
    /// otherwise incorrect one falls back to a bounded `endstream` search, as described
    /// by [`Self::recover_stream_range`].
    ///
    /// # Errors
    ///
    /// Returns an error if no structurally plausible terminator is present or `endobj`
    /// is missing.
    pub fn recover_stream_body(
        &mut self,
        length: Option<usize>,
    ) -> Result<Range<usize>, ParserError> {
        let body = self.recover_stream_range(length)?;
        self.consume_required_endobj()?;
        Ok(body)
    }

    /// Requires `endobj`, unless the current input is a safe malformed-file recovery boundary.
    pub(crate) fn consume_endobj_or_implicit_boundary(&mut self) -> Result<(), ParserError> {
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
    pub(crate) fn consume_required_endobj(&mut self) -> Result<(), ParserError> {
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
    use pdf_object_reader::{EmptySource, ObjectReader, object_variant::ObjectVariant};

    use super::*;

    /// Reads the stream header that follows the identifier at the start of `parser`.
    fn stream_header(parser: &mut PdfParser<'_>) -> StreamHeader {
        let identifier = parser
            .parse_indirect_object_id()
            .expect("indirect object header should parse");
        match parser.parse_indirect_object_value(identifier).unwrap() {
            IndirectValue::Stream(header) => header,
            IndirectValue::Object(object) => panic!("expected a stream, found {object:?}"),
        }
    }

    fn parse_staged_indirect_object(
        parser: &mut PdfParser<'_>,
    ) -> Result<Option<(ObjectId, ObjectVariant)>, ParserError> {
        let Some(identifier) = parser.parse_indirect_object_id() else {
            return Ok(None);
        };
        let object = match parser.parse_indirect_object_value(identifier)? {
            IndirectValue::Object(object) => object,
            IndirectValue::Stream(header) => {
                let length = ObjectReader::new(EmptySource)
                    .required::<usize>(header.dictionary(), b"Length")?;
                let body = parser.parse_stream_body(length)?;
                header.into_stream_in(&Bytes::copy_from_slice(parser.tokenizer.input), body)?
            }
        };
        Ok(Some((identifier, object)))
    }

    #[test]
    fn test_indirect_object_valid() {
        let input = b"0 1 obj\n(HELLO)\nendobj\n";
        let mut parser = PdfParser::from(input.as_slice());
        if let Some((identifier, object)) = parse_staged_indirect_object(&mut parser).unwrap() {
            assert_eq!(identifier.number, 0);
            assert_eq!(identifier.generation, 1);
            assert_eq!(
                object,
                pdf_object_reader::pdf_string::PdfString::from(
                    b"HELLO",
                    pdf_object_reader::string_kind::StringKind::Literal
                )
            );
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
            ObjectId {
                number: 12,
                generation: 3,
            }
        );
        assert_eq!(parser.parse_object().unwrap(), ObjectVariant::Integer(42));
    }

    #[test]
    fn test_direct_parser_only_consumes_number_from_indirect_object_declaration() {
        let mut parser = PdfParser::from(b"12 3 obj\n42\nendobj".as_slice());

        assert_eq!(parser.parse_object().unwrap(), ObjectVariant::Integer(12));
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
    fn test_stream_indirect_object_requires_endobj() {
        let input = b"1 0 obj\n<< /Length 5 >>\nstream\nHello\nendstream\n";
        let mut parser = PdfParser::from(input.as_slice());

        let result = parse_staged_indirect_object(&mut parser);

        assert!(result.is_err());
    }

    #[test]
    fn recovering_stream_parser_repairs_zero_length_payload() {
        let input = b"1 0 obj\n<< /Length 0 /Filter [/ASCIIHexDecode /FlateDecode] >>\nstream\n789C730A51700D010003DD0150>\nendstream\nendobj";
        let mut parser = PdfParser::from(input.as_slice());
        let header = stream_header(&mut parser);

        let body = parser
            .recover_stream_body(Some(0))
            .expect("the structurally delimited stream should recover");
        let object = header
            .into_stream_in(&Bytes::from_static(input), body)
            .unwrap();
        let ObjectVariant::Stream(stream) = object else {
            panic!("expected recovered stream");
        };

        assert_eq!(stream.raw_data(), b"789C730A51700D010003DD0150>");
    }

    #[test]
    fn recovering_stream_parser_requires_endobj_after_candidate() {
        let input = b"1 0 obj\n<< /Length 0 >>\nstream\npayload\nendstream\nnot-endobj";
        let mut parser = PdfParser::from(input.as_slice());
        stream_header(&mut parser);

        let result = parser.recover_stream_body(Some(0));

        assert!(result.is_err());
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
