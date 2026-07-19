//! Double-click tracking for page-scoped annotations.

use std::time::{Duration, Instant};

use pdf_annotation_types::annotation_id::AnnotationId;

/// The most recent annotation click eligible for double-click recognition.
#[derive(Clone, Copy, Debug)]
struct PreviousClick {
    /// Page containing the clicked annotation.
    page_index: usize,
    /// Stable page-scoped annotation identifier.
    annotation_id: AnnotationId,
    /// Monotonic time at which the click occurred.
    timestamp: Instant,
}

/// Tracks one pending click without exposing timing details to the controller.
#[derive(Debug, Default)]
pub(super) struct ClickTracker {
    /// Click that may pair with the next press.
    previous: Option<PreviousClick>,
}

impl ClickTracker {
    /// Records a click and reports whether it completes a double click.
    pub(super) fn register(
        &mut self,
        page_index: usize,
        annotation_id: AnnotationId,
        timestamp: Instant,
        interval: Duration,
    ) -> bool {
        let double_click = self.previous.is_some_and(|click| {
            click.page_index == page_index
                && click.annotation_id == annotation_id
                && timestamp.duration_since(click.timestamp) <= interval
        });
        self.previous = Some(PreviousClick {
            page_index,
            annotation_id,
            timestamp,
        });
        double_click
    }

    /// Discards any click awaiting a partner.
    pub(super) fn clear(&mut self) {
        self.previous = None;
    }
}
