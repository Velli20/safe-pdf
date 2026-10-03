use std::collections::HashSet;

/// Maximum number of content streams that may be active at the same time.
///
/// The value is derived from PDFium. PDFium applies that limit while parsing nested
/// Form XObjects; Safe-PDF applies the same numeric limit while rendering any
/// nested content stream, including Forms, Type 3 glyph procedures, and tiling
/// patterns.
const MAX_CONTENT_STREAM_DEPTH: usize = 40;

/// Tracks the content streams active during rendering and refuses cycles.
#[derive(Default)]
pub(crate) struct ContentStreamRenderState {
    /// Stable IDs for streams that currently have an admitted invocation.
    active_ids: HashSet<usize>,
}

/// Records the stream whose active entry must be released when an invocation finishes.
pub(crate) struct ContentStreamInvocation {
    /// Stable ID of the admitted content stream.
    stream_id: usize,
}

impl ContentStreamRenderState {
    /// Admits an invocation unless it nests too deeply or re-enters an active stream.
    ///
    /// Like PDFium for Form XObjects, a stream that is already being drawn is not drawn
    /// again inside itself. A cycle through Forms, Type 3 glyphs, or tiling patterns
    /// otherwise multiplies work at every level where it branches.
    pub(crate) fn enter(&mut self, stream_id: usize) -> Option<ContentStreamInvocation> {
        // Every admitted stream is active, so the set size is the current depth.
        if self.active_ids.len() >= MAX_CONTENT_STREAM_DEPTH || !self.active_ids.insert(stream_id) {
            return None;
        }
        Some(ContentStreamInvocation { stream_id })
    }

    /// Releases the active entry acquired by [`Self::enter`].
    ///
    /// Reusing a stream after its invocation has finished is ordinary rendering.
    pub(crate) fn exit(&mut self, invocation: ContentStreamInvocation) {
        let _ = self.active_ids.remove(&invocation.stream_id);
    }
}
