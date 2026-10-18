//! Hiding of marked-content sequences whose optional content is off.
use pdf_optional_content::{OptionalContentMembership, OptionalContentState};

/// Hides content in marked-content sequences whose optional content is off.
///
/// Only the outermost hiding sequence is tracked: everything nested inside hidden
/// content is hidden too, so inner sequences need no visibility lookup.
pub(crate) struct OptionalContentFilter<'a> {
    /// Group visibility the filter applies; at least one group is off.
    state: &'a OptionalContentState,
    /// Number of open marked-content sequences.
    depth: usize,
    /// Depth of the outermost open sequence that hides its content.
    hidden_from: Option<usize>,
}

impl<'a> OptionalContentFilter<'a> {
    /// Creates a filter for `state`, or `None` when every group is on and no content
    /// can be hidden.
    pub(crate) fn new(state: &'a OptionalContentState) -> Option<Self> {
        state.hides_any().then_some(Self {
            state,
            depth: 0,
            hidden_from: None,
        })
    }

    /// Returns a filter over the same state with no open sequences, for a nested canvas.
    pub(crate) fn restarted(&self) -> Self {
        Self {
            state: self.state,
            depth: 0,
            hidden_from: None,
        }
    }

    /// Opens a marked-content sequence tagged `tag`.
    ///
    /// `content` names the optional content an `OC` sequence belongs to; it is only
    /// consulted while nothing is hidden yet.
    pub(crate) fn begin<'c>(
        &mut self,
        tag: &[u8],
        content: impl FnOnce() -> Option<&'c OptionalContentMembership>,
    ) {
        self.depth = self.depth.saturating_add(1);
        if self.hidden_from.is_none()
            && tag == b"OC"
            && content().is_some_and(|content| !self.state.is_visible(content))
        {
            self.hidden_from = Some(self.depth);
        }
    }

    /// Closes the innermost open sequence; an unbalanced close is ignored.
    pub(crate) fn end(&mut self) {
        if self.hidden_from == Some(self.depth) {
            self.hidden_from = None;
        }
        self.depth = self.depth.saturating_sub(1);
    }

    /// Returns whether an open sequence hides the content being drawn.
    pub(crate) fn is_hidden(&self) -> bool {
        self.hidden_from.is_some()
    }

    /// Returns the number of open sequences.
    pub(crate) fn depth(&self) -> usize {
        self.depth
    }

    /// Closes the sequences opened beyond `depth`, such as those a content stream left open.
    pub(crate) fn close_to(&mut self, depth: usize) {
        if self
            .hidden_from
            .is_some_and(|hidden_from| hidden_from > depth)
        {
            self.hidden_from = None;
        }
        self.depth = self.depth.min(depth);
    }
}
