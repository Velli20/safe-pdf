//! Public value types used by annotation interaction.

use std::time::{Duration, Instant};

use pdf_graphics::{color::Color, point::Point};
use thiserror::Error;

use crate::{FreeTextEditError, WidgetEditError, interaction_viewport::AnnotationViewport};

/// Semantic editing input understood by [`crate::AnnotationController`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnnotationEditCommand<'a> {
    /// Insert printable text at the caret.
    Insert {
        /// Text to insert into the active edit buffer.
        text: &'a str,
    },
    /// Insert an explicit line break.
    Newline,
    /// Move the caret one character left.
    MoveLeft,
    /// Move the caret one character right.
    MoveRight,
    /// Move the caret to the beginning of the text.
    MoveToStart,
    /// Move the caret to the end of the text.
    MoveToEnd,
    /// Delete the character before the caret.
    DeleteBackward,
    /// Delete the character after the caret.
    DeleteForward,
    /// Commit the current edit session.
    Commit,
    /// Restore the annotation to its state before editing began.
    Cancel,
}

/// The effect of an interaction input on the containing application.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AnnotationInteractionResult {
    /// Whether the input was consumed by annotation interaction.
    pub consumed: bool,
    /// Whether the application should redraw its view.
    pub redraw: bool,
}

impl AnnotationInteractionResult {
    /// The input did not affect annotation interaction.
    pub const IGNORED: Self = Self {
        consumed: false,
        redraw: false,
    };
    /// The input was consumed without changing visible state.
    pub const CONSUMED: Self = Self {
        consumed: true,
        redraw: false,
    };
    /// Visible state changed without consuming the input.
    pub const REDRAW: Self = Self {
        consumed: false,
        redraw: true,
    };
    /// The input was consumed and changed visible state.
    pub const CONSUMED_AND_REDRAW: Self = Self {
        consumed: true,
        redraw: true,
    };
}

/// Interaction timing and device-space overlay configuration.
#[derive(Clone, Copy, Debug)]
pub struct AnnotationControllerOptions {
    /// Maximum interval between clicks that begin free-text editing.
    pub double_click_interval: Duration,
    /// Outline color for a selected annotation.
    pub selection_color: Color,
    /// Outline color while editing free text.
    pub editing_color: Color,
    /// Selection outline width in device units.
    pub outline_width: f32,
    /// Free-text caret color.
    pub caret_color: Color,
    /// Fill color for selected listbox rows.
    pub listbox_selection_color: Color,
}

impl Default for AnnotationControllerOptions {
    /// Returns conventional interaction timing and high-contrast overlay settings.
    fn default() -> Self {
        Self {
            double_click_interval: Duration::from_millis(500),
            selection_color: Color::from_rgba(0.20, 0.48, 1.0, 0.95),
            editing_color: Color::from_rgba(1.0, 0.42, 0.10, 0.95),
            outline_width: 2.0,
            caret_color: Color::from_rgba(0.05, 0.05, 0.05, 1.0),
            listbox_selection_color: Color::from_rgba(0.20, 0.48, 1.0, 0.28),
        }
    }
}

/// A primary-pointer press expressed in device coordinates.
#[derive(Clone, Copy, Debug)]
pub struct AnnotationPointerPress {
    /// Index of the page receiving the press.
    pub page_index: usize,
    /// Mapping between the page and the current device viewport.
    pub viewport: AnnotationViewport,
    /// Press position in device coordinates.
    pub position: Point,
    /// Monotonic timestamp used for double-click recognition.
    pub timestamp: Instant,
}

/// Primary-pointer movement expressed in device coordinates.
#[derive(Clone, Copy, Debug)]
pub struct AnnotationPointerMove {
    /// Index of the page receiving the movement.
    pub page_index: usize,
    /// Mapping between the page and the current device viewport.
    pub viewport: AnnotationViewport,
    /// Current pointer position in device coordinates.
    pub position: Point,
}

/// Errors produced while applying annotation interaction.
#[derive(Debug, Error)]
pub enum AnnotationInteractionError {
    /// Free-text editing failed.
    #[error(transparent)]
    FreeText(#[from] FreeTextEditError),
    /// Widget editing failed.
    #[error(transparent)]
    Widget(#[from] WidgetEditError),
    /// An annotation disappeared during an active interaction.
    #[error("annotation {id} was not found")]
    AnnotationNotFound {
        /// The page-scoped annotation identifier.
        id: usize,
    },
}
