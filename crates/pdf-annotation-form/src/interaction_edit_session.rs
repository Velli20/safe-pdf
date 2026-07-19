//! Transactional free-text editing state.

use pdf_annotation_types::annotation_id::AnnotationId;
use pdf_document::page::PdfPage;
use pdf_graphics::rect::Rect;

use crate::{
    FreeText, FreeTextEditor,
    interaction_types::{AnnotationEditCommand, AnnotationInteractionError},
};

/// Whether an editing command keeps or closes its session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EditSessionAction {
    /// Keep the edit session active.
    Continue,
    /// Close the edit session after applying the command.
    Finish,
}

/// A character-indexed text buffer with a bounded caret.
#[derive(Clone, Debug)]
struct EditBuffer {
    /// Candidate annotation text.
    text: String,
    /// Caret position measured in Unicode scalar values.
    cursor: usize,
}

impl EditBuffer {
    /// Creates a buffer with its caret at the end of the supplied text.
    fn new(text: String) -> Self {
        let cursor = text.chars().count();
        Self { text, cursor }
    }

    /// Returns the buffered text.
    fn text(&self) -> &str {
        &self.text
    }

    /// Returns the current character cursor.
    fn cursor(&self) -> usize {
        self.cursor
    }

    /// Returns the number of characters in the buffer.
    fn character_count(&self) -> usize {
        self.text.chars().count()
    }

    /// Inserts text at the caret and advances by the inserted character count.
    fn insert(&mut self, inserted: &str) {
        let byte_index = self.cursor_byte_index();
        self.text.insert_str(byte_index, inserted);
        self.cursor = self.cursor.saturating_add(inserted.chars().count());
    }

    /// Deletes the character immediately before the caret when present.
    fn delete_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.cursor.saturating_sub(1);
        self.delete_character_range(start, self.cursor);
    }

    /// Deletes the character immediately after the caret when present.
    fn delete_forward(&mut self) {
        if self.cursor >= self.character_count() {
            return;
        }
        self.delete_character_range(self.cursor, self.cursor.saturating_add(1));
    }

    /// Moves the caret one character toward the start of the buffer.
    fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Moves the caret one character toward the end of the buffer.
    fn move_right(&mut self) {
        self.cursor = self.cursor.saturating_add(1).min(self.character_count());
    }

    /// Moves the caret to the beginning of the buffer.
    fn move_to_start(&mut self) {
        self.cursor = 0;
    }

    /// Moves the caret to the end of the buffer.
    fn move_to_end(&mut self) {
        self.cursor = self.character_count();
    }

    /// Deletes a half-open character range and places the caret at its start.
    fn delete_character_range(&mut self, start: usize, end: usize) {
        let start_byte = self.byte_index(start);
        let end_byte = self.byte_index(end);
        self.text.replace_range(start_byte..end_byte, "");
        self.cursor = start;
    }

    /// Returns the byte index corresponding to the current character cursor.
    fn cursor_byte_index(&self) -> usize {
        self.byte_index(self.cursor)
    }

    /// Converts a bounded character position into a UTF-8 byte index.
    fn byte_index(&self, character_index: usize) -> usize {
        self.text
            .chars()
            .take(character_index)
            .map(char::len_utf8)
            .fold(0, usize::saturating_add)
    }
}

/// Owns the original annotation snapshot and its live edit buffer.
#[derive(Debug)]
pub(super) struct FreeTextEditSession {
    /// Identifier of the annotation being edited.
    id: AnnotationId,
    /// Snapshot restored when the session is cancelled.
    original: FreeText,
    /// Candidate text and character cursor.
    buffer: EditBuffer,
    /// Caret rectangle derived from the accepted candidate.
    caret_rect: Rect,
    /// Whether the page differs from the original snapshot.
    dirty: bool,
}

impl FreeTextEditSession {
    /// Starts an edit session from the annotation's current editable state.
    pub(super) fn begin(
        page: &mut PdfPage,
        id: AnnotationId,
    ) -> Result<Self, AnnotationInteractionError> {
        let original = FreeTextEditor::new(page).get(id)?;
        let buffer = EditBuffer::new(original.text.clone());
        let caret_rect = original.caret_rect(buffer.cursor())?;
        Ok(Self {
            id,
            original,
            buffer,
            caret_rect,
            dirty: false,
        })
    }

    /// Returns the edited annotation identifier.
    pub(super) const fn id(&self) -> AnnotationId {
        self.id
    }

    /// Returns the current caret rectangle in page coordinates.
    pub(super) const fn caret_rect(&self) -> Rect {
        self.caret_rect
    }

    /// Applies one semantic command atomically to the session and page.
    pub(super) fn handle(
        &mut self,
        page: &mut PdfPage,
        command: AnnotationEditCommand<'_>,
    ) -> Result<EditSessionAction, AnnotationInteractionError> {
        match command {
            AnnotationEditCommand::Insert { text } => self.apply_text_change(page, |buffer| {
                buffer.insert(text);
            })?,
            AnnotationEditCommand::Newline => self.apply_text_change(page, |buffer| {
                buffer.insert("\n");
            })?,
            AnnotationEditCommand::MoveLeft => self.apply_cursor_move(EditBuffer::move_left)?,
            AnnotationEditCommand::MoveRight => self.apply_cursor_move(EditBuffer::move_right)?,
            AnnotationEditCommand::MoveToStart => {
                self.apply_cursor_move(EditBuffer::move_to_start)?;
            }
            AnnotationEditCommand::MoveToEnd => self.apply_cursor_move(EditBuffer::move_to_end)?,
            AnnotationEditCommand::DeleteBackward => {
                self.apply_text_change(page, EditBuffer::delete_backward)?;
            }
            AnnotationEditCommand::DeleteForward => {
                self.apply_text_change(page, EditBuffer::delete_forward)?;
            }
            AnnotationEditCommand::Commit => return Ok(EditSessionAction::Finish),
            AnnotationEditCommand::Cancel => {
                self.restore_original(page)?;
                return Ok(EditSessionAction::Finish);
            }
        }
        Ok(EditSessionAction::Continue)
    }

    /// Validates and applies a candidate text mutation without partial updates.
    fn apply_text_change(
        &mut self,
        page: &mut PdfPage,
        change: impl FnOnce(&mut EditBuffer),
    ) -> Result<(), AnnotationInteractionError> {
        let mut buffer = self.buffer.clone();
        change(&mut buffer);
        if buffer.text() == self.buffer.text() && buffer.cursor() == self.buffer.cursor() {
            return Ok(());
        }

        let mut candidate = self.original.clone();
        candidate.text = buffer.text().to_owned();
        let caret_rect = candidate.caret_rect(buffer.cursor())?;
        FreeTextEditor::new(page).update(self.id, candidate)?;
        self.buffer = buffer;
        self.caret_rect = caret_rect;
        self.dirty = true;
        Ok(())
    }

    /// Recalculates the caret after a non-mutating cursor movement.
    fn apply_cursor_move(
        &mut self,
        movement: impl FnOnce(&mut EditBuffer),
    ) -> Result<(), AnnotationInteractionError> {
        let mut buffer = self.buffer.clone();
        movement(&mut buffer);
        let mut candidate = self.original.clone();
        candidate.text = buffer.text().to_owned();
        let caret_rect = candidate.caret_rect(buffer.cursor())?;
        self.buffer = buffer;
        self.caret_rect = caret_rect;
        Ok(())
    }

    /// Restores the pre-edit snapshot when the live annotation changed.
    fn restore_original(&self, page: &mut PdfPage) -> Result<(), AnnotationInteractionError> {
        if self.dirty {
            FreeTextEditor::new(page).update(self.id, self.original.clone())?;
        }
        Ok(())
    }
}
