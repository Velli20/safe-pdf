//! Paints annotation appearance streams: widgets over rendered page content, text-entry
//! widgets with their live value, and stamp, note and free-text artwork into a viewport
//! cropped to the annotation.
use crate::{PdfRenderer, PdfRendererError};
use pdf_annotation_core::PaintedAppearance;
use pdf_canvas::{PageViewport, canvas_backend::CanvasBackend, pdf_canvas::PdfCanvas};
use pdf_document::page::PdfPage;
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_optional_content::OptionalContentState;
use pdf_resources::{form::FormXObject, resources::Resources};
use std::sync::Arc;

impl PdfRenderer {
    /// Paints `appearances` over page `page_index`, mapping the page through `viewport`.
    ///
    /// Call after the page content pass on the same backend. Glyphs are not recorded,
    /// so appearance text stays out of the page's selectable text. An appearance whose
    /// stream cannot be read is skipped without affecting the others.
    ///
    /// # Errors
    /// Returns an error if the page is missing or drawing an appearance fails.
    pub fn render_annotation_appearances<B: CanvasBackend>(
        &self,
        canvas_backend: &mut B,
        page_index: usize,
        viewport: &PageViewport,
        optional_content: &OptionalContentState,
        appearances: &[PaintedAppearance],
    ) -> Result<(), PdfRendererError> {
        if appearances.is_empty() {
            return Ok(());
        }
        let page = self.page(page_index)?;
        let mut canvas = PdfCanvas::new_with_viewport(
            canvas_backend,
            page,
            viewport,
            Arc::clone(&self.font_system),
        )?
        .with_image_cache(Arc::clone(&self.image_cache))
        .with_optional_content(optional_content)
        .with_fallback_fonts(self.form_resources().cloned());
        for appearance in appearances {
            let Some(form) = self.appearance_form(appearance) else {
                continue;
            };
            let Some(matrix) = appearance_matrix(&form, &appearance.rect) else {
                continue;
            };
            let resources = self.appearance_resources(&form, page)?;
            let value = appearance
                .text
                .as_ref()
                .zip(form.bbox)
                .and_then(|(text, bbox)| {
                    self.variable_text_stream(
                        &form.content_stream,
                        bbox,
                        resources.as_deref(),
                        text,
                    )
                });
            canvas.render_content_stream(
                value.as_ref().unwrap_or(&form.content_stream),
                Some(matrix),
                form.bbox.as_ref(),
                resources,
                None,
            )?;
        }
        Ok(())
    }

    /// The resources `form` is drawn with on `page`: its own, else the page's.
    pub(crate) fn appearance_resources(
        &self,
        form: &FormXObject,
        page: &PdfPage,
    ) -> Result<Option<Arc<Resources>>, PdfRendererError> {
        Ok(form
            .resources
            .as_ref()
            .map(|resources| resources.get())
            .transpose()?
            .or_else(|| page.resources.clone()))
    }

    /// The interactive form's `/DR`, where widget appearances find fonts their own
    /// resources lack.
    pub(crate) fn form_resources(&self) -> Option<&Arc<Resources>> {
        self.document
            .interactive_form
            .as_ref()?
            .default_resources
            .as_ref()
    }

    /// Reads the form XObject of `appearance`'s stream.
    pub(crate) fn appearance_form(&self, appearance: &PaintedAppearance) -> Option<FormXObject> {
        self.document
            .objects()
            .read::<FormXObject>(appearance.stream.value())
            .ok()
    }
}

/// The form matrix composed with the map from its transformed `/BBox` onto `rect`.
///
/// Returns `None` when the transformed box has no area.
fn appearance_matrix(form: &FormXObject, rect: &Rect) -> Option<Transform> {
    let matrix = form.matrix.unwrap_or_default();
    let bbox = matrix.map_rect(form.bbox.as_ref()?);
    let (width, height) = (bbox.width(), bbox.height());
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let (sx, sy) = (rect.width() / width, rect.height() / height);
    let to_rect = Transform::from_row(
        sx,
        0.0,
        0.0,
        sy,
        rect.left - bbox.left * sx,
        rect.top - bbox.top * sy,
    );
    Some(to_rect.post_concatenated(&matrix))
}
