//! High-level orchestration of annotation interaction state.

use pdf_annotation_types::annotation_id::AnnotationId;
use pdf_canvas::{canvas_backend::CanvasBackend, error::PdfCanvasError};
use pdf_document::{document::PdfDocument, page::PdfPage};

use crate::{
    interaction_annotation_target::AnnotationTarget,
    interaction_click_tracker::ClickTracker,
    interaction_drag_session::{DragSession, DragUpdate},
    interaction_edit_session::{EditSessionAction, FreeTextEditSession},
    interaction_hit::AnnotationHit,
    interaction_hit_tester::AnnotationHitTester,
    interaction_overlay::OverlayRenderer,
    interaction_types::{
        AnnotationControllerOptions, AnnotationEditCommand, AnnotationInteractionError,
        AnnotationInteractionResult, AnnotationPointerMove, AnnotationPointerPress,
    },
    interaction_viewport::AnnotationViewport,
};

/// Exclusive mode currently owned by the annotation controller.
#[derive(Debug, Default)]
enum ControllerMode {
    /// No drag or free-text edit session is active.
    #[default]
    Idle,
    /// A primary-pointer drag is pending or active.
    Dragging {
        /// Gesture-specific drag state.
        session: DragSession,
    },
    /// A free-text edit session is active.
    Editing {
        /// Transactional free-text state.
        session: FreeTextEditSession,
    },
}

/// Owns reusable annotation selection and free-text interaction state.
pub struct AnnotationController {
    /// Interaction timing and overlay presentation options.
    options: AnnotationControllerOptions,
    /// Selected page-scoped annotation identifier.
    selected: Option<AnnotationId>,
    /// Mutually exclusive drag and editing state.
    mode: ControllerMode,
    /// Pending click used for double-click recognition.
    clicks: ClickTracker,
}

impl Default for AnnotationController {
    /// Creates a controller with default interaction options.
    fn default() -> Self {
        Self::new(AnnotationControllerOptions::default())
    }
}

impl AnnotationController {
    /// Creates a controller with explicit interaction options.
    pub fn new(options: AnnotationControllerOptions) -> Self {
        Self {
            options,
            selected: None,
            mode: ControllerMode::Idle,
            clicks: ClickTracker::default(),
        }
    }

    /// Returns the selected page-scoped annotation identifier.
    pub const fn selected(&self) -> Option<AnnotationId> {
        self.selected
    }

    /// Returns whether a free-text edit session is active.
    pub const fn is_editing(&self) -> bool {
        matches!(self.mode, ControllerMode::Editing { .. })
    }

    /// Clears transient state when the containing application changes pages.
    pub fn page_changed(&mut self) -> AnnotationInteractionResult {
        let redraw = self.selected.take().is_some() || self.is_editing();
        self.clicks.clear();
        self.mode = ControllerMode::Idle;
        if redraw {
            AnnotationInteractionResult::REDRAW
        } else {
            AnnotationInteractionResult::IGNORED
        }
    }

    /// Handles a primary pointer press described by named device-space input.
    pub fn pointer_pressed(
        &mut self,
        document: &mut PdfDocument,
        press: AnnotationPointerPress,
    ) -> Result<AnnotationInteractionResult, AnnotationInteractionError> {
        self.stop_dragging();
        let page =
            document
                .pages
                .get(press.page_index)
                .ok_or(crate::WidgetEditError::PageNotFound {
                    page_index: press.page_index,
                })?;
        let hit = AnnotationHitTester::new(page, press.viewport).hit_at(press.position);
        self.finish_editing_when_clicking_elsewhere(hit);
        let Some(hit) = hit else {
            return Ok(self.clear_selection());
        };
        self.handle_annotation_press(document, press, hit)
    }

    /// Handles primary-pointer movement described by named device-space input.
    pub fn pointer_moved(
        &mut self,
        document: &mut PdfDocument,
        movement: AnnotationPointerMove,
    ) -> Result<AnnotationInteractionResult, AnnotationInteractionError> {
        let ControllerMode::Dragging { session } = &mut self.mode else {
            return Ok(AnnotationInteractionResult::IGNORED);
        };
        let update = session.update(document, movement)?;
        if matches!(update, DragUpdate::Active | DragUpdate::Redraw) {
            self.clicks.clear();
        }
        Ok(match update {
            DragUpdate::Ignored => AnnotationInteractionResult::IGNORED,
            DragUpdate::Pending | DragUpdate::Active => AnnotationInteractionResult::CONSUMED,
            DragUpdate::Redraw => AnnotationInteractionResult::CONSUMED_AND_REDRAW,
        })
    }

    /// Ends the current primary-pointer drag gesture.
    pub fn pointer_released(&mut self) -> AnnotationInteractionResult {
        if self.stop_dragging() {
            AnnotationInteractionResult::CONSUMED
        } else {
            AnnotationInteractionResult::IGNORED
        }
    }

    /// Applies a semantic editing command to the active free-text session.
    pub fn handle_edit_command(
        &mut self,
        page: &mut PdfPage,
        command: AnnotationEditCommand<'_>,
    ) -> Result<AnnotationInteractionResult, AnnotationInteractionError> {
        let ControllerMode::Editing { session } = &mut self.mode else {
            return Ok(AnnotationInteractionResult::IGNORED);
        };
        if matches!(session.handle(page, command)?, EditSessionAction::Finish) {
            self.mode = ControllerMode::Idle;
        }
        Ok(AnnotationInteractionResult::CONSUMED_AND_REDRAW)
    }

    /// Draws selection and caret overlays through any canvas backend.
    pub fn draw_overlay<B: CanvasBackend>(
        &self,
        backend: &mut B,
        page: &PdfPage,
        viewport: AnnotationViewport,
    ) -> Result<(), PdfCanvasError> {
        OverlayRenderer::new(
            &self.options,
            self.selected,
            self.is_editing(),
            self.caret_rect(),
        )
        .draw(backend, page, viewport)
    }

    /// Selects and activates the annotation represented by a pointer hit.
    fn handle_annotation_press(
        &mut self,
        document: &mut PdfDocument,
        press: AnnotationPointerPress,
        hit: AnnotationHit,
    ) -> Result<AnnotationInteractionResult, AnnotationInteractionError> {
        let id = hit.id();
        self.selected = Some(id);
        if hit.activate(document, press.page_index)? {
            self.clicks.clear();
            return Ok(AnnotationInteractionResult::CONSUMED_AND_REDRAW);
        }

        let page = document.pages.get_mut(press.page_index).ok_or(
            crate::WidgetEditError::PageNotFound {
                page_index: press.page_index,
            },
        )?;
        let Some(target) = AnnotationTarget::from_page(page, id) else {
            return Err(AnnotationInteractionError::AnnotationNotFound { id: id.get() });
        };
        self.update_click_state(page, press, id, target)?;
        self.begin_drag_if_available(press, id, target);
        Ok(AnnotationInteractionResult::CONSUMED_AND_REDRAW)
    }

    /// Starts editing on a qualifying double click and records every click.
    fn update_click_state(
        &mut self,
        page: &mut PdfPage,
        press: AnnotationPointerPress,
        id: AnnotationId,
        target: AnnotationTarget,
    ) -> Result<(), AnnotationInteractionError> {
        let double_click = self.clicks.register(
            press.page_index,
            id,
            press.timestamp,
            self.options.double_click_interval,
        );
        if double_click && target.is_free_text() && self.editing_id() != Some(id) {
            let session = FreeTextEditSession::begin(page, id)?;
            self.mode = ControllerMode::Editing { session };
        }
        Ok(())
    }

    /// Starts a pending drag unless free-text editing took precedence.
    fn begin_drag_if_available(
        &mut self,
        press: AnnotationPointerPress,
        id: AnnotationId,
        target: AnnotationTarget,
    ) {
        if self.is_editing() {
            return;
        }
        if let Some(original_rect) = target.draggable_rect() {
            self.mode = ControllerMode::Dragging {
                session: DragSession::new(
                    press.page_index,
                    id,
                    press.position,
                    original_rect,
                    press.viewport,
                ),
            };
        }
    }

    /// Commits an edit session implicitly when another annotation is clicked.
    fn finish_editing_when_clicking_elsewhere(&mut self, hit: Option<AnnotationHit>) {
        let hit_id = hit.map(AnnotationHit::id);
        if self.editing_id() != hit_id && self.is_editing() {
            self.mode = ControllerMode::Idle;
        }
    }

    /// Clears selection and click history after an empty-space press.
    fn clear_selection(&mut self) -> AnnotationInteractionResult {
        let redraw = self.selected.take().is_some();
        self.clicks.clear();
        if redraw {
            AnnotationInteractionResult::REDRAW
        } else {
            AnnotationInteractionResult::IGNORED
        }
    }

    /// Returns the identifier owned by the active edit session.
    fn editing_id(&self) -> Option<AnnotationId> {
        match &self.mode {
            ControllerMode::Editing { session } => Some(session.id()),
            ControllerMode::Idle | ControllerMode::Dragging { .. } => None,
        }
    }

    /// Returns the page-space caret rectangle for overlay rendering.
    fn caret_rect(&self) -> Option<pdf_graphics::rect::Rect> {
        match &self.mode {
            ControllerMode::Editing { session } => Some(session.caret_rect()),
            ControllerMode::Idle | ControllerMode::Dragging { .. } => None,
        }
    }

    /// Stops a pending or active drag and reports whether one existed.
    fn stop_dragging(&mut self) -> bool {
        if matches!(self.mode, ControllerMode::Dragging { .. }) {
            self.mode = ControllerMode::Idle;
            true
        } else {
            false
        }
    }
}
