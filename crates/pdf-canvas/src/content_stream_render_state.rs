use std::collections::HashSet;

/// Maximum number of content streams that may be active at the same time.
///
/// The value is derived from PDFium. PDFium applies that limit while parsing nested
/// Form XObjects; Safe-PDF applies the same numeric limit while rendering any
/// nested content stream, including Forms, Type 3 glyph procedures, and tiling
/// patterns.
const MAX_CONTENT_STREAM_DEPTH: usize = 40;

/// Maximum number of recursive content-stream invocations admitted by a canvas.
///
/// The value is derived from PDFium, which
/// bounded the total number of nested Form XObjects expanded from one outer Form.
/// Safe-PDF adapts that safeguard to count invocations whose stable content-stream
/// ID is already active. This specifically bounds branching cycles without
/// penalizing ordinary nesting through distinct streams.
const MAX_RECURSIVE_CONTENT_STREAM_INVOCATIONS: usize = 4096;

/// Tracks the content streams active during rendering and bounds recursive work.
#[derive(Clone, Default)]
pub(crate) struct ContentStreamRenderState {
    /// Stable IDs for streams that currently have an admitted invocation.
    active_ids: HashSet<usize>,
    /// Number of content-stream invocations currently active on the call stack.
    depth: usize,
    /// Cumulative number of admitted invocations whose stream ID was already active.
    recursive_invocations: usize,
}

/// Records the state changes that must be undone when an invocation finishes.
pub(crate) struct ContentStreamInvocation {
    /// Stable ID of the admitted content stream.
    stream_id: usize,
    /// Whether this invocation inserted `stream_id` into the active-ID set.
    owns_active_id: bool,
}

impl ContentStreamRenderState {
    /// Admits an invocation when it is within both recursion safeguards.
    pub(crate) fn enter(&mut self, stream_id: usize) -> Option<ContentStreamInvocation> {
        // The depth limit bounds a single chain of nested streams. The separate
        // invocation budget is needed because a recursive stream can branch and
        // perform exponentially more work without exceeding that depth.
        if self.depth >= MAX_CONTENT_STREAM_DEPTH {
            return None;
        }

        // A stream is recursive only when the same stable ID is already on the
        // active call stack. Reusing a stream after its outermost invocation has
        // completed is ordinary rendering and does not consume this budget.
        let is_recursive = self.active_ids.contains(&stream_id);

        // Count only repeated active IDs. Distinct nested streams are already
        // bounded by the depth limit and may legitimately form a deep hierarchy.
        if is_recursive && self.recursive_invocations >= MAX_RECURSIVE_CONTENT_STREAM_INVOCATIONS {
            return None;
        }

        if is_recursive {
            // The counter is cumulative for this render state so a branching
            // cycle cannot evade the budget by repeatedly unwinding shallow calls.
            self.recursive_invocations = self.recursive_invocations.saturating_add(1);
        }

        // `insert` returns true only for the first active invocation of this ID.
        // That invocation becomes responsible for removing the ID during exit;
        // recursive invocations must leave their outer owner's entry intact.
        let owns_active_id = self.active_ids.insert(stream_id);

        // Increment depth only after both guards admit the invocation. This keeps
        // rejected streams from consuming capacity or requiring cleanup.
        self.depth = self.depth.saturating_add(1);

        Some(ContentStreamInvocation {
            stream_id,
            owns_active_id,
        })
    }

    /// Releases the depth and active-ID state acquired by [`Self::enter`].
    pub(crate) fn exit(&mut self, invocation: ContentStreamInvocation) {
        self.depth = self.depth.saturating_sub(1);

        // A recursive invocation did not insert the ID, so only the outermost
        // owner removes it after all nested work for that stream has unwound.
        if invocation.owns_active_id {
            let _ = self.active_ids.remove(&invocation.stream_id);
        }
    }
}
