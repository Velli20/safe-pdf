//! Iteration over visible listbox rows.

use pdf_graphics::rect::Rect;

use crate::interaction_listbox_row::ListboxRow;

/// Iterator over visible listbox rows without allocating intermediate storage.
pub(super) struct VisibleListboxRows {
    /// Index assigned to the next row.
    next_index: usize,
    /// Exclusive upper option bound.
    option_count: usize,
    /// Device-space top coordinate of the next row.
    row_top: f32,
    /// Widget rectangle that clips every row.
    rect: Rect,
    /// Height advanced after each row.
    row_height: f32,
}

impl VisibleListboxRows {
    /// Creates an iterator over rows beginning at the top option index.
    pub(super) const fn new(
        top_index: usize,
        option_count: usize,
        rect: Rect,
        row_height: f32,
    ) -> Self {
        Self {
            next_index: top_index,
            option_count,
            row_top: rect.top,
            rect,
            row_height,
        }
    }
}

impl Iterator for VisibleListboxRows {
    type Item = ListboxRow;

    /// Advances to the next visible, clipped row.
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.option_count || self.row_top >= self.rect.bottom {
            return None;
        }
        let option_index = self.next_index;
        let top = self.row_top;
        self.next_index = self.next_index.saturating_add(1);
        self.row_top += self.row_height;
        Some(ListboxRow {
            option_index,
            rect: Rect {
                left: self.rect.left,
                top,
                right: self.rect.right,
                bottom: self.row_top.min(self.rect.bottom),
            },
        })
    }
}
