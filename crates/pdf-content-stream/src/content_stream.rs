use pdf_content_stream_operators::variants::PdfOperatorVariant;

/// Represents one materialized PDF content stream as parsed operators plus its
/// stable content-stream ID.
pub struct ContentStream {
    /// The parsed drawing operators from the content stream.
    pub operators: Vec<PdfOperatorVariant>,
    /// A monotonic ID assigned when this content stream is materialized.
    pub id: usize,
}

#[cfg(test)]
mod tests {
    #[test]
    fn content_stream_holds_operators_and_id() {
        let content_stream = crate::content_stream::ContentStream {
            operators: Vec::new(),
            id: 7,
        };

        assert!(content_stream.operators.is_empty());
        assert_eq!(content_stream.id, 7);
    }
}
