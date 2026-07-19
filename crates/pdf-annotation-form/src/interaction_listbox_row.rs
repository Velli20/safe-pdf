//! Visible listbox row geometry.

use pdf_graphics::rect::Rect;

/// One visible listbox option and its clipped device-space rectangle.
#[derive(Clone, Copy, Debug)]
pub(super) struct ListboxRow {
    /// Index into the widget's complete option list.
    pub(super) option_index: usize,
    /// Visible row bounds clipped to the widget rectangle.
    pub(super) rect: Rect,
}
