//! Whether and how a live annotation is shown.
use crate::{kind::AnnotationKind, models::Annotation};
use pdf_optional_content::OptionalContentState;

impl Annotation {
    /// Whether this annotation is shown: live flags permit display, the payload has
    /// geometry to place, and `optional_content` does not hide it. Unknown subtypes
    /// without usable bounds are never shown.
    pub fn is_displayed(&self, optional_content: &OptionalContentState) -> bool {
        // Optional content hides an annotation without altering its flags, so the two
        // conditions are independent: either one alone suppresses display.
        self.metadata.is_visible()
            && !matches!(&self.content, AnnotationKind::Unknown(v) if v.bounds.is_none())
            && self
                .optional_content()
                .is_none_or(|content| optional_content.is_visible(content))
    }

    /// Whether this annotation's appearance is painted at its fixed source rectangle instead
    /// of a native presentation: source ink without strokes, and caret or unknown subtypes
    /// that carry a normal appearance.
    pub fn is_drawn_from_appearance(&self) -> bool {
        match &self.content {
            AnnotationKind::Ink(ink) => ink.strokes.is_empty(),
            AnnotationKind::Caret(_) | AnnotationKind::Unknown(_) => self.has_normal_appearance(),
            _ => false,
        }
    }
}
