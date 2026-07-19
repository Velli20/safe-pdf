//! Rendering of selection, listbox, and caret overlays.

use pdf_annotation_types::{Annotation, AnnotationKind, annotation_id::AnnotationId};
use pdf_canvas::{canvas_backend::CanvasBackend, error::PdfCanvasError, stroke_style::StrokeStyle};
use pdf_document::page::PdfPage;
use pdf_graphics::{BlendMode, PathFillType, pdf_path::PdfPath, rect::Rect};

use crate::{
    AnnotationControllerOptions, interaction_listbox_layout::ListboxLayout,
    interaction_viewport::AnnotationViewport,
};

/// Immutable interaction state needed to render device-space overlays.
pub(super) struct OverlayRenderer<'a> {
    /// Overlay styling and dimensions.
    options: &'a AnnotationControllerOptions,
    /// Currently selected annotation.
    selected: Option<AnnotationId>,
    /// Whether the selected annotation is being edited.
    editing: bool,
    /// Active caret rectangle in page coordinates.
    caret_rect: Option<Rect>,
}

impl<'a> OverlayRenderer<'a> {
    /// Creates an overlay renderer from a controller snapshot.
    pub(super) const fn new(
        options: &'a AnnotationControllerOptions,
        selected: Option<AnnotationId>,
        editing: bool,
        caret_rect: Option<Rect>,
    ) -> Self {
        Self {
            options,
            selected,
            editing,
            caret_rect,
        }
    }

    /// Draws listbox selection, annotation outline, and caret overlays.
    pub(super) fn draw<B: CanvasBackend>(
        &self,
        backend: &mut B,
        page: &PdfPage,
        viewport: AnnotationViewport,
    ) -> Result<(), PdfCanvasError> {
        self.draw_listbox_selections(backend, page, viewport)?;
        self.draw_selection_outline(backend, page, viewport)?;
        self.draw_caret(backend, viewport)
    }

    /// Draws selected rows for every visible listbox annotation.
    fn draw_listbox_selections<B: CanvasBackend>(
        &self,
        backend: &mut B,
        page: &PdfPage,
        viewport: AnnotationViewport,
    ) -> Result<(), PdfCanvasError> {
        let Some(annotations) = page.annotations.as_deref() else {
            return Ok(());
        };
        for annotation in annotations {
            self.draw_annotation_listbox_selection(backend, annotation, viewport)?;
        }
        Ok(())
    }

    /// Draws selected rows belonging to one listbox annotation.
    fn draw_annotation_listbox_selection<B: CanvasBackend>(
        &self,
        backend: &mut B,
        annotation: &Annotation,
        viewport: AnnotationViewport,
    ) -> Result<(), PdfCanvasError> {
        let AnnotationKind::Widget(widget) = &annotation.kind else {
            return Ok(());
        };
        let Some(layout) = ListboxLayout::from_annotation(annotation, viewport) else {
            return Ok(());
        };
        let selected = widget.selected_option_indices();
        for row in layout.rows() {
            if selected.contains(&row.option_index) {
                backend.fill_path(
                    &PdfPath::from(&row.rect),
                    PathFillType::Winding,
                    self.options.listbox_selection_color,
                    &None,
                    Some(BlendMode::Normal),
                )?;
            }
        }
        Ok(())
    }

    /// Draws the selected annotation's device-space outline.
    fn draw_selection_outline<B: CanvasBackend>(
        &self,
        backend: &mut B,
        page: &PdfPage,
        viewport: AnnotationViewport,
    ) -> Result<(), PdfCanvasError> {
        let Some(rect) = self
            .selected
            .and_then(|id| page.annotation(id))
            .and_then(|annotation| annotation.rect.as_ref())
            .and_then(|rect| viewport.map_rect(rect))
        else {
            return Ok(());
        };
        let color = if self.editing {
            self.options.editing_color
        } else {
            self.options.selection_color
        };
        backend.stroke_path(
            &PdfPath::from(&rect),
            color,
            self.options.outline_width,
            &StrokeStyle::default(),
            &None,
            Some(BlendMode::Normal),
        )
    }

    /// Draws the active free-text caret when it maps into device space.
    fn draw_caret<B: CanvasBackend>(
        &self,
        backend: &mut B,
        viewport: AnnotationViewport,
    ) -> Result<(), PdfCanvasError> {
        let Some(rect) = self.caret_rect.and_then(|rect| viewport.map_rect(&rect)) else {
            return Ok(());
        };
        backend.fill_path(
            &PdfPath::from(&rect),
            PathFillType::Winding,
            self.options.caret_color,
            &None,
            Some(BlendMode::Normal),
        )
    }
}
