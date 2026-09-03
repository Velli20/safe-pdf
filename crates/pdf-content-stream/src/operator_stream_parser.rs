use pdf_object::object_resolver::PassthroughResolver;
use pdf_parser::error::ParserError;
use pdf_parser::parser::PdfParser;
use pdf_tokenizer::error::TokenizerError;

use pdf_content_stream_operators::{
    error::PdfOperatorError,
    operands::Operands,
    operation_map::{OpDescriptor, get_operation_descriptor},
    variants::PdfOperatorVariant,
};

/// Incrementally parses one content stream into PDF operators while reusing the
/// same operand buffer across operator boundaries.
pub(super) struct OperatorStreamParser<'a, 'out> {
    parser: PdfParser<'a>,
    operands: Operands,
    out: &'out mut Vec<PdfOperatorVariant>,
}

impl<'a, 'out> OperatorStreamParser<'a, 'out> {
    /// Creates a parser for a single borrowed content stream.
    ///
    /// The operand buffer is preallocated because PDF operators typically have
    /// a small fixed number of operands.
    pub(super) fn new(input: &'a [u8], out: &'out mut Vec<PdfOperatorVariant>) -> Self {
        Self {
            parser: PdfParser::from(input),
            operands: Operands::with_capacity(6),
            out,
        }
    }

    /// Parses the next operand or operator from the stream.
    ///
    /// Returns `Ok(false)` when the stream is exhausted and `Ok(true)` after
    /// successfully consuming one item.
    pub(super) fn parse_next_item(&mut self) -> Result<bool, PdfOperatorError> {
        self.skip_trivia();

        let Some(next_byte) = self.peek_next_byte() else {
            return Ok(false);
        };

        let start_position = self.parser.position();
        let result = if next_item_is_operator(next_byte) {
            self.parse_operator_from_stream().map(|()| true)
        } else {
            self.parse_operand_or_stop(next_byte)
        };

        match result {
            Ok(parsed) => Ok(parsed),
            Err(error) if is_truncated_operand_error(&error) => {
                self.operands.clear();
                Ok(false)
            }
            Err(_) => {
                self.recover_after_malformed_item(start_position);
                Ok(true)
            }
        }
    }

    /// Skips inter-token whitespace and comments before reading the next item.
    fn skip_trivia(&mut self) {
        self.parser.skip_whitespace_and_comments();
    }

    /// Returns the next raw byte without advancing the parser.
    fn peek_next_byte(&self) -> Option<u8> {
        self.parser.peek_byte()
    }

    /// Parses one operand object and appends it to the reusable operand buffer.
    fn parse_operand(&mut self, first_byte: u8) -> Result<(), PdfOperatorError> {
        let value = if matches!(first_byte, b'+' | b'-' | b'.' | b'0'..=b'9') {
            self.parser.parse_number()?
        } else {
            self.parser.parse_object(&PassthroughResolver)?
        };
        self.operands.push(value);
        Ok(())
    }

    /// Parses one operand object or stops cleanly when the stream ends mid-object.
    fn parse_operand_or_stop(&mut self, first_byte: u8) -> Result<bool, PdfOperatorError> {
        match self.parse_operand(first_byte) {
            Ok(()) => Ok(true),
            Err(error) if is_truncated_operand_error(&error) => {
                self.operands.clear();
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    /// Parses the next operator token and dispatches it through the operation map.
    fn parse_operator_from_stream(&mut self) -> Result<(), PdfOperatorError> {
        let descriptor = {
            let name = self.read_operator_name()?;
            get_operation_descriptor(name)
        };

        let Some(descriptor) = descriptor else {
            self.operands.clear();
            return Ok(());
        };

        let parsed = if let Some(parse_hook) = descriptor.parse_hook {
            if let Some(operator) = parse_hook(&mut self.parser)? {
                Some(operator)
            } else {
                parse_operator(&descriptor, &mut self.operands)?
            }
        } else {
            parse_operator(&descriptor, &mut self.operands)?
        };

        self.push_if_parsed(parsed);
        self.operands.clear();
        Ok(())
    }

    /// Reads a single operator token and normalizes low-level tokenization
    /// failures into content-stream operator errors.
    fn read_operator_name(&mut self) -> Result<&[u8], PdfOperatorError> {
        Ok(self.parser.read_operator_name()?)
    }

    /// Appends a parsed operator when dispatch produced one.
    fn push_if_parsed(&mut self, operator: Option<PdfOperatorVariant>) {
        if let Some(operator) = operator {
            self.out.push(operator);
        }
    }

    /// Drops malformed syntax and guarantees the parser can try the next byte.
    fn recover_after_malformed_item(&mut self, start_position: usize) {
        self.operands.clear();

        if self.parser.position() <= start_position {
            let _ = self.parser.read_byte();
        }
    }
}

/// Returns whether a regular-character token starts an operator rather than a
/// numeric operand.
fn next_item_is_operator(byte: u8) -> bool {
    PdfParser::is_pdf_regular_character(byte) && !matches!(byte, b'+' | b'-' | b'.' | b'0'..=b'9')
}

/// Returns whether an operand parse error means the content stream ended
/// before the operand object was fully available.
fn is_truncated_operand_error(error: &PdfOperatorError) -> bool {
    match error {
        PdfOperatorError::ParserError(parser_error) => match parser_error {
            ParserError::UnexpectedEndOfFile => true,
            ParserError::TokenizerError(tokenizer_error) => {
                matches!(
                    tokenizer_error,
                    TokenizerError::UnexpectedEndOfFile(_, _)
                        | TokenizerError::UnexpectedToken(None, _)
                )
            }
            _ => false,
        },
        PdfOperatorError::TokenizerError(tokenizer_error) => {
            matches!(
                tokenizer_error,
                TokenizerError::UnexpectedEndOfFile(_, _)
                    | TokenizerError::UnexpectedToken(None, _)
            )
        }
        _ => false,
    }
}

/// Parses a single operator with its operands.
///
/// Looks up the operator descriptor by name and validates the operand count
/// before parsing. The operand buffer retains its allocation for the next
/// operator, including when parsing fails.
fn parse_operator(
    descriptor: &OpDescriptor,
    operands: &mut Operands,
) -> Result<Option<PdfOperatorVariant>, PdfOperatorError> {
    if let Some(required_count) = descriptor.operand_count
        && operands.len() != required_count
    {
        return Ok(None);
    }

    let operator = (descriptor.parser)(operands)?;
    Ok(Some(operator))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use pdf_content_stream_operators::{
        graphics_state_operators::RestoreGraphicsState, variants::PdfOperatorVariant,
    };
    use pdf_object::object_variant::ObjectVariant;

    use super::*;

    #[test]
    fn next_item_is_operator_rejects_numeric_prefixes() {
        for byte in [b'+', b'-', b'.', b'0', b'9'] {
            assert!(
                !next_item_is_operator(byte),
                "byte {byte:?} should be an operand"
            );
        }

        for byte in [b'q', b'B', b'*', b'@'] {
            assert!(
                next_item_is_operator(byte),
                "byte {byte:?} should be an operator"
            );
        }
    }

    #[test]
    fn truncated_operand_errors_are_detected() {
        for error in [
            PdfOperatorError::ParserError(ParserError::UnexpectedEndOfFile),
            PdfOperatorError::ParserError(ParserError::TokenizerError(
                TokenizerError::UnexpectedToken(None, pdf_tokenizer::PdfToken::RightSquareBracket),
            )),
            PdfOperatorError::TokenizerError(TokenizerError::UnexpectedEndOfFile(1, 0)),
        ] {
            assert!(is_truncated_operand_error(&error));
        }

        assert!(!is_truncated_operand_error(&PdfOperatorError::ParserError(
            ParserError::UnexpectedTokenAt {
                token: "]".to_string(),
                position: 0,
            },
        )));
    }

    #[test]
    fn parse_operator_reuses_operand_buffer_after_success() {
        let descriptor = get_operation_descriptor(b"m").expect("operator descriptor should exist");
        let mut operands =
            Operands::from(vec![ObjectVariant::Integer(10), ObjectVariant::Integer(20)]);
        let original_capacity = operands.capacity();

        let operator = parse_operator(&descriptor, &mut operands).expect("operator should parse");

        assert!(matches!(operator, Some(PdfOperatorVariant::MoveTo(_))));
        assert!(operands.is_empty());
        assert!(operands.capacity() >= original_capacity);
    }

    #[test]
    fn parse_operator_skips_malformed_fixed_arity_operator_without_consuming_buffer() {
        let descriptor = get_operation_descriptor(b"m").expect("operator descriptor should exist");
        let mut operands = Operands::from(vec![
            ObjectVariant::Integer(1),
            ObjectVariant::Integer(2),
            ObjectVariant::Integer(3),
        ]);
        let original_capacity = operands.capacity();

        let operator = parse_operator(&descriptor, &mut operands)
            .expect("malformed operator should be skipped");

        assert!(operator.is_none());
        assert_eq!(operands.len(), 3);
        assert_eq!(operands.peek_next(), Some(&ObjectVariant::Integer(1)));
        assert_eq!(operands.capacity(), original_capacity);
    }

    #[test]
    fn parse_next_item_skips_unknown_operator_and_clears_pending_operands() {
        let mut out = Vec::new();
        let mut parser = OperatorStreamParser::new(b"unknown q", &mut out);
        parser.operands.push(ObjectVariant::Integer(1));

        assert!(
            parser
                .parse_next_item()
                .expect("unknown operators should be skipped")
        );

        assert!(parser.operands.is_empty());
        assert!(parser.out.is_empty());

        assert!(
            parser
                .parse_next_item()
                .expect("parser should continue after unknown operator")
        );
        assert!(matches!(
            parser.out.first(),
            Some(PdfOperatorVariant::SaveGraphicsState(_))
        ));
    }

    #[test]
    fn parse_next_item_uses_parse_hook_and_resumes_after_inline_image() {
        let mut out = Vec::new();
        let mut parser =
            OperatorStreamParser::new(b"BI /W 1 /H 1 /BPC 8 /CS /G ID x\nEI Q", &mut out);

        assert!(
            parser
                .parse_next_item()
                .expect("inline image should parse through hook")
        );
        assert_eq!(parser.out.len(), 1);
        let cloned = parser
            .out
            .first()
            .cloned()
            .expect("parsed operator should be present");
        match (parser.out.first(), &cloned) {
            (
                Some(PdfOperatorVariant::InlineImage(image)),
                PdfOperatorVariant::InlineImage(cloned_image),
            ) => {
                assert_eq!(image.shared_data().as_ref(), b"x\n");
                assert!(std::rc::Rc::ptr_eq(image, cloned_image));
            }
            other => panic!("expected inline image, got {other:?}"),
        }

        assert!(
            parser
                .parse_next_item()
                .expect("should parse the next operator")
        );
        assert!(matches!(
            parser.out.get(1),
            Some(PdfOperatorVariant::RestoreGraphicsState(
                RestoreGraphicsState
            ))
        ));
    }

    #[test]
    fn parse_next_item_stops_cleanly_on_truncated_trailing_operand() {
        let mut out = Vec::new();
        let mut parser = OperatorStreamParser::new(b"0 j 0 J [ ", &mut out);

        while parser
            .parse_next_item()
            .expect("stream items before truncation should parse")
        {}

        assert!(parser.operands.is_empty());
        assert_eq!(parser.out.len(), 2);
        assert!(matches!(
            parser.out.first(),
            Some(PdfOperatorVariant::SetLineJoinStyle(_))
        ));
        assert!(matches!(
            parser.out.get(1),
            Some(PdfOperatorVariant::SetLineCapStyle(_))
        ));
    }

    #[test]
    fn parse_next_item_recovers_from_leading_invalid_delimiter() {
        let mut out = Vec::new();
        let mut parser = OperatorStreamParser::new(b") q", &mut out);

        while parser
            .parse_next_item()
            .expect("invalid delimiter should be skipped")
        {}

        assert!(parser.operands.is_empty());
        assert_eq!(parser.out.len(), 1);
        assert!(matches!(
            parser.out.first(),
            Some(PdfOperatorVariant::SaveGraphicsState(_))
        ));
    }

    #[test]
    fn parse_next_item_recovers_from_corrupt_operand_before_valid_operator() {
        let mut out = Vec::new();
        let mut parser = OperatorStreamParser::new(b"[ ) 0 J", &mut out);

        while parser
            .parse_next_item()
            .expect("corrupt operand should be skipped")
        {}

        assert!(parser.operands.is_empty());
        assert_eq!(parser.out.len(), 1);
        assert!(matches!(
            parser.out.first(),
            Some(PdfOperatorVariant::SetLineCapStyle(_))
        ));
    }

    #[test]
    fn numeric_operands_recover_from_top_level_reference_syntax() {
        let mut out = Vec::new();
        let mut parser = OperatorStreamParser::new(b"1 0 R q", &mut out);

        while parser
            .parse_next_item()
            .expect("invalid top-level reference should be skipped")
        {}

        assert_eq!(parser.out.len(), 1);
        assert!(matches!(
            parser.out.first(),
            Some(PdfOperatorVariant::SaveGraphicsState(_))
        ));
    }

    #[test]
    fn generic_operands_keep_nested_indirect_references() {
        let mut out = Vec::new();
        let mut parser = OperatorStreamParser::new(b"/Tag << /Ref 5 0 R >> BDC EMC", &mut out);

        while parser
            .parse_next_item()
            .expect("nested reference should parse in a dictionary")
        {}

        assert_eq!(parser.out.len(), 2);
        assert!(matches!(
            parser.out.first(),
            Some(PdfOperatorVariant::BeginMarkedContentWithProps(_))
        ));
        assert!(matches!(
            parser.out.get(1),
            Some(PdfOperatorVariant::EndMarkedContent(_))
        ));
    }

    #[test]
    fn parse_next_item_recovers_from_invalid_operator_operand_value() {
        let mut out = Vec::new();
        let mut parser = OperatorStreamParser::new(b"9 J q", &mut out);
        let original_capacity = parser.operands.capacity();

        while parser
            .parse_next_item()
            .expect("invalid operator operand should be skipped")
        {}

        assert!(parser.operands.is_empty());
        assert_eq!(parser.operands.capacity(), original_capacity);
        assert_eq!(parser.out.len(), 1);
        assert!(matches!(
            parser.out.first(),
            Some(PdfOperatorVariant::SaveGraphicsState(_))
        ));
    }

    #[test]
    fn parse_next_item_recovers_from_malformed_inline_image() {
        let mut out = Vec::new();
        let mut parser = OperatorStreamParser::new(b"BI /W 1 /H 1 ID abc Q", &mut out);

        while parser
            .parse_next_item()
            .expect("malformed inline image should be skipped")
        {}

        assert!(parser.operands.is_empty());
        assert_eq!(parser.out.len(), 1);
        assert!(matches!(
            parser.out.first(),
            Some(PdfOperatorVariant::RestoreGraphicsState(_))
        ));
    }
}
