//! Content-stream bytes read without parsing, and the parse over them.
use std::ops::ControlFlow;

use bytes::Bytes;
use pdf_content_stream_operators::variants::PdfOperatorVariant;
use pdf_object_reader::{
    EmptySource, FromPdfObject, ObjectAccess, ObjectContext, ObjectReader, ReadResult, ReadSession,
    object_kind::ObjectKind, object_variant::ObjectVariant, stream::StreamObject,
};

use crate::operator_stream_parser::OperatorStreamParser;

/// The bytes of a content stream, or of an array of streams that form one, with the
/// content-stream ID of this read.
///
/// Reading it parses no operators. [`Self::for_each_batch`] parses them in order and
/// hands them over in small batches, so a consumer that draws operators once never
/// holds them all.
pub struct ContentStreamSource {
    /// The bytes of each present stream, in order, shared with the stream objects.
    streams: Vec<Bytes>,
    /// The content-stream ID assigned when this source was read.
    id: usize,
}

/// Reads a stream or an array of streams within the active object traversal.
///
/// Object resolution and shape errors propagate, and consume no content-stream ID.
/// Each successful read allocates one ID, including an empty stream array.
impl FromPdfObject for ContentStreamSource {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        if context.object().kind() != ObjectKind::Array {
            let context = context.stream()?;
            let streams = vec![context.stream().shared_data()];
            let id = context.content_stream_ids().next_id()?;
            return Ok(Self { streams, id });
        }
        let mut context = context.array()?;
        let elements = context.array();
        let mut streams = Vec::with_capacity(elements.len());
        for (index, element) in elements.iter().enumerate() {
            // Null and dangling entries contribute no content (ISO 32000 §7.3.10).
            if context.is_absent(element)? {
                continue;
            }
            streams.push(context.at::<StreamObject>(index)?.shared_data());
        }
        // The combined content receives one ID after every entry succeeds.
        let id = context.content_stream_ids().next_id()?;
        Ok(Self { streams, id })
    }
}

impl ContentStreamSource {
    /// The content-stream ID assigned when this source was read.
    #[must_use]
    pub fn id(&self) -> usize {
        self.id
    }

    /// Parses the operators best-effort, handing them to `apply` in order, a batch at a
    /// time, as soon as each batch is parsed.
    ///
    /// Malformed syntax is skipped as when collecting the operators, so `apply` sees the
    /// operators a [`crate::ContentStream`] read from the same object would hold.
    ///
    /// # Errors
    ///
    /// Stops parsing at the first error from `apply` and returns it.
    pub fn for_each_batch<E>(
        &self,
        mut apply: impl FnMut(&[PdfOperatorVariant]) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut failure = None;
        self.parse_batches(&mut |operators| {
            let applied = apply(operators);
            operators.clear();
            match applied {
                Ok(()) => ControlFlow::Continue(()),
                Err(error) => {
                    failure = Some(error);
                    ControlFlow::Break(())
                }
            }
        });
        failure.map_or(Ok(()), Err)
    }

    /// Parses all operators best-effort into one list.
    pub(crate) fn collect_operators(&self) -> Vec<PdfOperatorVariant> {
        let mut collected = Vec::new();
        self.parse_batches(&mut |operators| {
            collected.append(operators);
            ControlFlow::Continue(())
        });
        collected
    }

    /// Parses the operators, handing each batch to `take`, which must empty the batch.
    /// Parsing stops at the first batch `take` refuses.
    fn parse_batches(&self, take: &mut BatchTaker<'_>) {
        let reader = ObjectReader::new(EmptySource);
        let mut parsed = ParsedOperators {
            session: reader.session(),
            operands: Vec::with_capacity(6),
            operators: Vec::new(),
            take,
        };
        // A refused batch only ends the parse; the refusal reaches the caller through `take`.
        let _stopped = parsed.parse_streams(&self.streams);
    }
}

/// Takes a batch of parsed operators, leaving the batch empty; a break stops parsing.
type BatchTaker<'take> = dyn FnMut(&mut Vec<PdfOperatorVariant>) -> ControlFlow<()> + 'take;

/// How many operators are parsed before they are handed over, which keeps parsing and
/// drawing each in a tight loop.
const STREAM_BATCH: usize = 256;

/// The operators parsed from a content stream and not yet handed over, and the operands
/// awaiting theirs.
struct ParsedOperators<'reader, 'take> {
    session: ReadSession<'reader, EmptySource>,
    /// Reused across operators. Preallocated because PDF operators typically have a
    /// small fixed number of operands.
    operands: Vec<ObjectVariant>,
    operators: Vec<PdfOperatorVariant>,
    /// Takes the parsed operators every [`STREAM_BATCH`] operators and at the end of each
    /// parsed input.
    take: &'take mut BatchTaker<'take>,
}

impl ParsedOperators<'_, '_> {
    /// Parses a content stream held in one stream or several.
    fn parse_streams(&mut self, streams: &[Bytes]) -> ControlFlow<()> {
        let [stream] = streams else {
            return self.parse_stream_array(streams);
        };
        self.parse(stream, false)?;
        ControlFlow::Continue(())
    }

    /// Parses the streams of an array, which form one content stream (ISO 32000 §7.8.2).
    ///
    /// Streams divide only at token boundaries, which need not fall between operations,
    /// so an item can continue into the next stream. A newline separates them, which also
    /// ends a trailing comment. Each stream is parsed in place until an item may continue
    /// past its end; from that item on, the remaining content is joined and parsed once,
    /// which keeps an item that never ends from being parsed again for every stream.
    fn parse_stream_array(&mut self, streams: &[Bytes]) -> ControlFlow<()> {
        let mut joined = Vec::new();
        for (index, stream) in streams.iter().enumerate() {
            let unparsed = if joined.is_empty() {
                let more_input_follows = index.saturating_add(1) < streams.len();
                self.parse(stream, more_input_follows)?
            } else {
                Some(stream.as_ref())
            };
            if let Some(unparsed) = unparsed {
                joined.extend_from_slice(unparsed);
                joined.push(b'\n');
            }
        }
        self.parse(&joined, false)?;
        ControlFlow::Continue(())
    }

    /// Parses bytes best-effort, preserving operators recovered around malformed syntax.
    ///
    /// When `more_input_follows` is set, continues with the input from the first item that
    /// later content may continue, leaving that item unparsed. Breaks when `take` refuses
    /// a batch.
    fn parse<'a>(
        &mut self,
        input: &'a [u8],
        more_input_follows: bool,
    ) -> ControlFlow<(), Option<&'a [u8]>> {
        let mut parser = OperatorStreamParser::new(
            input,
            more_input_follows,
            std::mem::take(&mut self.operands),
            &mut self.session,
            &mut self.operators,
        );
        // One loop keeps `parse_next_item` inlined into it; a second call site outlines it.
        let mut taken = ControlFlow::Continue(());
        while parser.parse_next_item() {
            if parser.parsed().len() >= STREAM_BATCH {
                taken = (self.take)(parser.parsed());
                if taken.is_break() {
                    break;
                }
            }
        }
        if taken.is_continue() {
            taken = (self.take)(parser.parsed());
        }
        let unfinished = parser.unfinished();
        self.operands = parser.into_operands();
        taken?;
        ControlFlow::Continue(unfinished)
    }
}
