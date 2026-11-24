//! Append-only storage for the verbs of many paths.

use std::ops::Range;

use pdf_graphics::pdf_path::PathVerb;

/// Verbs the first chunk of a buffer has room for.
const FIRST_CHUNK_VERBS: usize = 16;

/// Verbs a chunk has room for at most, unless it holds a single longer path.
const MAX_CHUNK_VERBS: usize = 1024;

/// Verbs of many paths, stored in chunks that never move once written.
///
/// Chunks grow up to a fixed size, so a small buffer stays small and a large one never
/// copies what it already holds. A path's verbs are contiguous within one chunk.
///
/// A chunk is closed when the next path does not fit, so the room it leaves unused is
/// smaller than that path. All closed chunks together waste less than the verbs stored.
#[derive(Clone, Default)]
pub(crate) struct VerbBuffer {
    /// Chunks in recording order. Only the last one is open.
    chunks: Vec<Vec<PathVerb>>,
}

/// Position of one path's verbs in a [`VerbBuffer`].
#[derive(Clone)]
pub(crate) struct VerbRange {
    /// Index of the chunk holding the verbs.
    chunk: usize,
    /// Position of the verbs in the chunk.
    range: Range<usize>,
}

/// End of a [`VerbBuffer`] at one point in time.
pub(crate) struct VerbMark {
    /// Number of chunks.
    chunks: usize,
    /// Number of verbs in the last chunk.
    last_len: usize,
}

impl VerbBuffer {
    /// Appends a copy of `verbs` and returns where it is stored.
    pub(crate) fn push(&mut self, verbs: &[PathVerb]) -> VerbRange {
        let chunk = match self.chunks.last_mut() {
            Some(chunk) if verbs.len() <= chunk.capacity().saturating_sub(chunk.len()) => chunk,
            last => {
                let grown = last.map_or(FIRST_CHUNK_VERBS, |chunk| {
                    chunk.capacity().saturating_mul(2).min(MAX_CHUNK_VERBS)
                });
                self.chunks
                    .push_mut(Vec::with_capacity(grown.max(verbs.len())))
            }
        };
        let start = chunk.len();
        chunk.extend_from_slice(verbs);
        let range = start..chunk.len();
        VerbRange {
            chunk: self.chunks.len().saturating_sub(1),
            range,
        }
    }

    /// Returns the verbs stored at `position`, or `None` if it lies outside the buffer.
    pub(crate) fn get(&self, position: &VerbRange) -> Option<&[PathVerb]> {
        self.chunks.get(position.chunk)?.get(position.range.clone())
    }

    /// Returns the current end of the buffer.
    pub(crate) fn mark(&self) -> VerbMark {
        VerbMark {
            chunks: self.chunks.len(),
            last_len: self.chunks.last().map_or(0, Vec::len),
        }
    }

    /// Removes every verb appended after `mark` was taken.
    pub(crate) fn truncate(&mut self, mark: &VerbMark) {
        self.chunks.truncate(mark.chunks);
        if let Some(last) = self.chunks.last_mut() {
            last.truncate(mark.last_len);
        }
    }
}
