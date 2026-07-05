use pdf_annotation_types::Annotation;
use pdf_canvas::{canvas_backend::CanvasBackend, error::PdfCanvasError, pdf_canvas::PdfCanvas};
use pdf_graphics::{rect::Rect, transform::Transform};
use thiserror::Error;

use crate::AppearanceField;

/// The interactive annotation appearance state to render.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum AnnotationInteractionState {
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
    /// Canvas used to render annotations and surrounding page content.
    pub canvas: PdfCanvas<'a, B>,
    interaction_state: AnnotationInteractionState,
}

impl<'a, B: CanvasBackend> AnnotationRenderer<'a, B> {
    /// Creates a renderer around an existing PDF canvas.
    pub fn new(canvas: PdfCanvas<'a, B>) -> Self {
        Self {
            canvas,
            interaction_state: AnnotationInteractionState::Normal,
        }
    }

    /// Creates a renderer around an existing PDF canvas for an interaction state.
    pub fn with_interaction_state(
        canvas: PdfCanvas<'a, B>,
        interaction_state: AnnotationInteractionState,
    ) -> Self {
        Self {
            canvas,
            interaction_state,
        }
    }

    /// Returns the wrapped canvas after annotation rendering is complete.
    pub fn into_canvas(self) -> PdfCanvas<'a, B> {
        self.canvas
    }

    /// Renders annotations in document order.
    pub fn render_annotations(
        &mut self,
        annotations: &'a [Annotation],
    ) -> Result<(), AnnotationRenderError> {
        for annotation in annotations {
            self.render_annotation_current_state(annotation)?;
        }
        Ok(())
    }

    /// Renders annotations in document order with per-annotation interaction states.
    pub fn render_annotations_with_state_resolver<F>(
        &mut self,
        annotations: &'a [Annotation],
        mut resolver: F,
    ) -> Result<(), AnnotationRenderError>
    where
        F: FnMut(usize, &Annotation) -> AnnotationInteractionState,
    {
        let previous_state = self.interaction_state;

        let result = (|| {
            for (index, annotation) in annotations.iter().enumerate() {
                self.interaction_state = resolver(index, annotation);
                self.render_annotation_current_state(annotation)?;
            }
            Ok(())
        })();

        self.interaction_state = previous_state;
        result
    }

    /// Renders one annotation.
    pub fn render_annotation(
        &mut self,
        annotation: &'a Annotation,
    ) -> Result<(), AnnotationRenderError> {
        self.render_annotation_current_state(annotation)
    }

    /// Renders one annotation for an interaction state.
    pub fn render_annotation_with_state(
        &mut self,
        annotation: &'a Annotation,
        interaction_state: AnnotationInteractionState,
    ) -> Result<(), AnnotationRenderError> {
        let previous_state = self.interaction_state;
        self.interaction_state = interaction_state;

        let result = self.render_annotation_current_state(annotation);

        self.interaction_state = previous_state;
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

        let requested = match self.interaction_state {
            AnnotationInteractionState::Normal => appearance_dictionary.normal.as_ref(),
            AnnotationInteractionState::Rollover => appearance_dictionary.rollover.as_ref(),
            AnnotationInteractionState::Down => appearance_dictionary.down.as_ref(),
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
