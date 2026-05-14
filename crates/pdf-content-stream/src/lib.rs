pub mod content_stream;
mod content_stream_id_allocator;
mod operator_stream_parser;

pub use content_stream::ContentStream;
pub use content_stream_id_allocator::ContentStreamIdAllocator;
use operator_stream_parser::OperatorStreamParser;
use pdf_content_stream_operators::{error::PdfOperatorError, variants::PdfOperatorVariant};
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant, stream::StreamObject,
};

pub use pdf_content_stream_operators::{operands::Operands, operator_trait::PdfOperator};

/// Parses one decoded PDF content stream into a newly allocated operator list.
///
/// This is the simplest entry point for callers that already have the raw bytes
/// of a single content stream and want an owned `Vec` of parsed
/// [`PdfOperatorVariant`] values.
///
/// Parsing is intentionally tolerant in the same way as the lower-level stream
/// parser:
///
/// - unknown operator tokens are skipped
/// - malformed fixed-arity operators are skipped when their operand count does
///   not match
/// - trailing truncated operand objects stop parsing cleanly instead of turning
///   the whole stream into an error
/// - operators with custom parsing hooks, such as inline images, may consume
///   additional bytes directly from the input stream
///
/// The returned vector contains only successfully parsed operators, in the same
/// order they appeared in `input`.
///
/// # Parameters
///
/// - `input`: The decoded bytes of a single PDF content stream.
///
/// # Returns
///
/// Returns a newly allocated vector containing the parsed operators.
///
/// # Errors
///
/// Returns [`PdfOperatorError`] when parsing encounters a non-recoverable
/// tokenizer, parser, object, or operand-conversion failure. Recoverable cases
/// such as unknown operators and certain malformed/truncated trailing input are
/// skipped rather than returned as errors.
pub fn parse(input: &[u8]) -> Result<Vec<PdfOperatorVariant>, PdfOperatorError> {
    let mut operators = Vec::new();
    parse_into(input, &mut operators)?;
    Ok(operators)
}

/// Parses one decoded PDF content stream and appends its operators into an
/// existing output buffer.
///
/// This variant is intended for callers that want to reuse a single allocation
/// across multiple streams, such as when a page `/Contents` entry is represented
/// as an array of separate stream objects.
///
/// The function never clears `out`. Newly parsed operators are appended after
/// any operators already present in the vector.
///
/// Parsing behavior matches [`parse`]:
///
/// - unknown operators are skipped
/// - malformed fixed-arity operators are skipped when the collected operand
///   count does not match
/// - truncated trailing operands terminate parsing cleanly
/// - custom operator hooks may consume additional bytes from the underlying
///   stream
///
/// # Parameters
///
/// - `input`: The decoded bytes of a single PDF content stream.
/// - `out`: Destination buffer that receives parsed operators in source order.
///
/// # Returns
///
/// Returns `Ok(())` after all recoverable items in `input` have been processed
/// and all successfully parsed operators have been appended to `out`.
///
/// # Errors
///
/// Returns [`PdfOperatorError`] when parsing encounters a non-recoverable
/// failure. In that case, `out` may already contain operators parsed before the
/// failing item, and those successfully appended values are preserved.
pub(crate) fn parse_into(
    input: &[u8],
    out: &mut Vec<PdfOperatorVariant>,
) -> Result<(), PdfOperatorError> {
    let mut parser = OperatorStreamParser::new(input, out);
    while parser.parse_next_item()? {}

    Ok(())
}

/// Resolves and parses a page dictionary's `/Contents` entry into a materialized
/// [`ContentStream`].
///
/// The `/Contents` entry may legally be absent, a single stream, or an array of
/// streams. This function handles all three cases:
///
/// - if `/Contents` is missing, it returns `Ok(None)`
/// - if `/Contents` resolves to a stream, that stream is parsed directly
/// - if `/Contents` resolves to an array, each referenced stream payload is
///   decoded, concatenated with a single newline byte between adjacent payloads,
///   and then parsed as one logical stream
///
/// The newline separator is inserted to prevent tokens at stream boundaries from
/// merging accidentally when adjacent decoded payloads do not end and start with
/// separating whitespace.
///
/// A content-stream ID is allocated only when parsing succeeds and a
/// [`ContentStream`] is actually produced. Missing `/Contents` therefore does
/// not consume an ID from `id_allocator`.
///
/// # Parameters
///
/// - `dictionary`: The page or form-like dictionary containing an optional
///   `/Contents` entry.
/// - `objects`: Resolver used to follow indirect references and materialize
///   stream objects.
/// - `id_allocator`: Monotonic allocator used to assign the returned
///   [`ContentStream::id`] when content exists and parses successfully.
///
/// # Returns
///
/// Returns:
///
/// - `Ok(None)` if the dictionary has no `/Contents` entry
/// - `Ok(Some(ContentStream))` if `/Contents` exists and parses successfully
///
/// # Errors
///
/// Returns [`PdfOperatorError`] if object resolution fails, if `/Contents`
/// resolves to a type other than `Stream` or `Array`, if any referenced stream
/// cannot be decoded, if parsing encounters a non-recoverable failure, or if
/// `id_allocator` is exhausted while assigning a new content-stream ID.
pub fn parse_content_stream_from_dictionary(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<Option<ContentStream>, PdfOperatorError> {
    const KEY: &str = "Contents";

    let Some(contents) = dictionary.get(KEY) else {
        return Ok(None);
    };

    let operators = match objects.resolve_object(contents)? {
        ObjectVariant::Stream(stream) => {
            let data = stream.data()?;
            parse(&data)?
        }
        ObjectVariant::Array(array_obj) => process_content_stream_array(array_obj, objects)?,
        other => {
            return Err(ObjectError::TypeMismatch("Stream or Array", other.name()).into());
        }
    };

    let id = id_allocator.next_id()?;
    Ok(Some(ContentStream { operators, id }))
}

/// Parses one already-resolved stream object into a materialized
/// [`ContentStream`].
///
/// This helper is intended for callers that have already resolved the relevant
/// [`StreamObject`] and only need to decode its bytes into operators plus assign
/// a fresh content-stream ID.
///
/// The stream payload is decoded through [`StreamObject::data`], parsed with
/// [`parse`], and then wrapped together with the next ID from `id_allocator`.
/// An ID is allocated only after the stream bytes have been decoded and parsed
/// successfully.
///
/// # Parameters
///
/// - `stream`: The resolved PDF stream object whose decoded bytes should be
///   parsed as content-stream operators.
/// - `id_allocator`: Monotonic allocator used to assign the returned
///   [`ContentStream::id`].
///
/// # Returns
///
/// Returns a fully materialized [`ContentStream`] containing the parsed
/// operators and its assigned ID.
///
/// # Errors
///
/// Returns [`PdfOperatorError`] if the stream payload cannot be decoded, if
/// parsing encounters a non-recoverable failure, or if `id_allocator` is
/// exhausted while assigning the content-stream ID.
pub fn parse_content_stream_from_stream(
    stream: &StreamObject,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<ContentStream, PdfOperatorError> {
    let data = stream.data()?;
    let operators = parse(&data)?;
    let id = id_allocator.next_id()?;
    Ok(ContentStream { operators, id })
}

fn process_content_stream_array(
    array: &[ObjectVariant],
    objects: &dyn ObjectResolver,
) -> Result<Vec<PdfOperatorVariant>, PdfOperatorError> {
    let mut combined_data = Vec::new();

    for value_in_array in array {
        let data = value_in_array.try_stream(objects)?.data()?;
        if !combined_data.is_empty() {
            combined_data.push(b'\n');
        }
        combined_data.extend_from_slice(&data);
    }

    parse(&combined_data)
}
