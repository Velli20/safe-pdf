use pdf_annotation_types::Annotation;
use pdf_canvas::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};
use pdf_graphics::{rect::Rect, transform::Transform};
use thiserror::Error;

use crate::AppearanceField;

/// The interactive annotation appearance state to render.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum AnnotationAppearanceState {
    /// Render the annotation's normal `/N` appearance.
    #[default]
    Normal,
    /// Render the annotation's rollover `/R` appearance, falling back to `/N`.
    Rollover,
    /// Render the annotation's down `/D` appearance, falling back to `/N`.
    Down,
}

/// Errors that can occur while rendering annotations.
#[derive(Debug, Error)]
pub enum AnnotationRenderError {
    /// Annotation drawing failed while issuing canvas operations.
    #[error("PDF canvas error: {0}")]
    Canvas(#[from] PdfCanvasError),
}

/// Renders parsed PDF annotation appearance streams directly into a [`PdfCanvas`].
///
/// Annotations without a usable `/Rect` or `/AP` appearance stream are ignored.
pub struct AnnotationRenderer<'a, B: CanvasBackend> {
    canvas: PdfCanvas<'a, B>,
    appearance_state: AnnotationAppearanceState,
}

impl<'a, B: CanvasBackend> AnnotationRenderer<'a, B> {
    /// Creates a renderer around an existing PDF canvas.
    pub fn new(canvas: PdfCanvas<'a, B>) -> Self {
        Self {
            canvas,
            appearance_state: AnnotationAppearanceState::Normal,
        }
    }

    /// Creates a renderer around an existing PDF canvas for an interaction state.
    pub fn with_interaction_state(
        canvas: PdfCanvas<'a, B>,
        appearance_state: AnnotationAppearanceState,
    ) -> Self {
        Self {
            canvas,
            appearance_state,
        }
    }

    /// Returns mutable access to the wrapped PDF canvas.
    pub fn canvas_mut(&mut self) -> &mut PdfCanvas<'a, B> {
        &mut self.canvas
    }

    /// Returns the wrapped canvas after annotation rendering is complete.
    pub fn into_canvas(self) -> PdfCanvas<'a, B> {
        self.canvas
    }

    /// Renders annotations in document order.
    pub fn render_all(
        &mut self,
        annotations: &'a [Annotation],
    ) -> Result<(), AnnotationRenderError> {
        for annotation in annotations {
            self.render_annotation_current_state(annotation)?;
        }
        Ok(())
    }

    /// Renders annotations in document order with per-annotation interaction states.
    pub fn render_all_with_state<F>(
        &mut self,
        annotations: &'a [Annotation],
        mut resolver: F,
    ) -> Result<(), AnnotationRenderError>
    where
        F: FnMut(&Annotation) -> AnnotationAppearanceState,
    {
        let previous_state = self.appearance_state;

        let result = (|| {
            for annotation in annotations {
                self.appearance_state = resolver(annotation);
                self.render_annotation_current_state(annotation)?;
            }
            Ok(())
        })();

        self.appearance_state = previous_state;
        result
    }

    /// Renders one annotation.
    pub fn render(&mut self, annotation: &'a Annotation) -> Result<(), AnnotationRenderError> {
        self.render_annotation_current_state(annotation)
    }

    /// Renders one annotation for an interaction state.
    pub fn render_with_state(
        &mut self,
        annotation: &'a Annotation,
        appearance_state: AnnotationAppearanceState,
    ) -> Result<(), AnnotationRenderError> {
        let previous_state = self.appearance_state;
        self.appearance_state = appearance_state;

        let result = self.render_annotation_current_state(annotation);

        self.appearance_state = previous_state;
        result
    }

    fn render_annotation_current_state(
        &mut self,
        annotation: &'a Annotation,
    ) -> Result<(), AnnotationRenderError> {
        self.render_selected_appearance(annotation)?;
        Ok(())
    }

    fn render_selected_appearance(
        &mut self,
        annotation: &'a Annotation,
    ) -> Result<bool, AnnotationRenderError> {
        let Some(appearance_dictionary) = annotation.appearance.as_ref() else {
            return Ok(false);
        };

        let requested = match self.appearance_state {
            AnnotationAppearanceState::Normal => appearance_dictionary.normal.as_ref(),
            AnnotationAppearanceState::Rollover => appearance_dictionary.rollover.as_ref(),
            AnnotationAppearanceState::Down => appearance_dictionary.down.as_ref(),
        };

        let Some(appearance) = AppearanceField::selected_appearance(
            requested,
            appearance_dictionary.normal.as_ref(),
            &annotation.appearance_state,
        ) else {
            return Ok(false);
        };

        let Some(annotation_rect) = annotation.rect.as_ref() else {
            return Ok(false);
        };

        let annotation_rect = annotation_rect.normalized();
        let appearance_bbox = appearance.bbox.normalized();
        let Some(placement) = appearance_placement_transform(&annotation_rect, &appearance_bbox)
        else {
            return Ok(false);
        };

        self.canvas.render_content_stream(
            &appearance.content_stream,
            Some(placement),
            Some(&appearance_bbox),
            appearance.resources.as_ref(),
            None,
        )?;

        Ok(true)
    }
}

fn appearance_placement_transform(rect: &Rect, bbox: &Rect) -> Option<Transform> {
    if !rect.is_valid() || !bbox.is_valid() {
        return None;
    }

    let scale_x = rect.width() / bbox.width();
    let scale_y = rect.height() / bbox.height();
    Some(Transform::from_row(
        scale_x,
        0.0,
        0.0,
        scale_y,
        rect.left - bbox.left * scale_x,
        rect.top - bbox.top * scale_y,
    ))
}
