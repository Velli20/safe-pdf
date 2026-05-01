use pdf_object::object_resolver::PassthroughResolver;
use pdf_parser::{error::ParserError, parser::PdfParser};

use crate::{error::PdfOperatorError, xobject_and_image_operators::InlineImage};

use super::variants::PdfOperatorVariant;

const INLINE_IMAGE_DATA_BEGIN: &[u8] = b"ID";
const INLINE_IMAGE_DATA_END: &[u8] = b"EI";

pub(crate) fn parse_inline_image(
    parser: &mut PdfParser<'_>,
    out: &mut Vec<PdfOperatorVariant>,
) -> Result<(), PdfOperatorError> {
    InlineImageReader::new(parser).parse_into(out)
}

struct InlineImageReader<'parser, 'input> {
    parser: &'parser mut PdfParser<'input>,
}

impl<'parser, 'input> InlineImageReader<'parser, 'input> {
    fn new(parser: &'parser mut PdfParser<'input>) -> Self {
        Self { parser }
    }

    fn parse_into(mut self, out: &mut Vec<PdfOperatorVariant>) -> Result<(), PdfOperatorError> {
        let dictionary = self
            .parser
            .parse_dictionary_until_keyword(&PassthroughResolver, INLINE_IMAGE_DATA_BEGIN)?;
        self.consume_required_data_separator()?;

        let data = self.read_data_until_end()?;
        out.push(PdfOperatorVariant::InlineImage(InlineImage::new(
            dictionary, data,
        )));

        Ok(())
    }

    fn consume_required_data_separator(&mut self) -> Result<(), PdfOperatorError> {
        let Some(first) = self.parser.tokenizer.data().first().copied() else {
            return Err(unexpected_end_of_file());
        };

        if !PdfParser::is_pdf_whitespace(first) {
            return Err(missing_delimiter_after_data_begin(
                first,
                self.parser.tokenizer.position,
            ));
        }

        if matches!(first, b'\r' | b'\n') {
            self.parser.try_read_end_of_line_marker();
        } else {
            let _ = self.parser.tokenizer.read_exactly(1)?;
        }

        Ok(())
    }

    fn read_data_until_end(&mut self) -> Result<Vec<u8>, PdfOperatorError> {
        let start = self.parser.tokenizer.position;
        let remaining = self.parser.tokenizer.data();
        let end = find_inline_image_data_end(remaining).ok_or_else(unexpected_end_of_file)?;
        let data = remaining.get(..end).unwrap_or(&[]).to_vec();

        self.parser.tokenizer.position = start + end + INLINE_IMAGE_DATA_END.len();
        Ok(data)
    }
}

fn find_inline_image_data_end(data: &[u8]) -> Option<usize> {
    for (offset, window) in data.windows(INLINE_IMAGE_DATA_END.len()).enumerate() {
        if window != INLINE_IMAGE_DATA_END {
            continue;
        }

        let previous = offset
            .checked_sub(1)
            .and_then(|index| data.get(index))
            .copied();
        let next = data.get(offset + INLINE_IMAGE_DATA_END.len()).copied();
        if previous.is_none_or(PdfParser::is_pdf_delimiter)
            && next.is_none_or(PdfParser::is_pdf_delimiter)
        {
            return Some(offset);
        }
    }

    None
}

fn unexpected_end_of_file() -> PdfOperatorError {
    PdfOperatorError::ParserError(ParserError::UnexpectedEndOfFile)
}

fn missing_delimiter_after_data_begin(found: u8, position: usize) -> PdfOperatorError {
    PdfOperatorError::ParserError(ParserError::MissingDelimiterAfterKeyword {
        keyword: String::from_utf8_lossy(INLINE_IMAGE_DATA_BEGIN).into_owned(),
        found,
        position,
    })
}
