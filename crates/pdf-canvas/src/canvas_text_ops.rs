use crate::canvas_backend::CanvasBackend;
use crate::error::PdfCanvasError;
use crate::pdf_canvas::PdfCanvas;
use pdf_content_stream_operators::pdf_operator_backend::{
    TextObjectOps, TextPositioningOps, TextShowingOps, TextStateOps,
};
use pdf_content_stream_operators::variants::PdfOperatorVariant;
use pdf_graphics::pdf_path::PdfPath;
use pdf_graphics::transform::Transform;
use pdf_graphics::{PaintMode, PathFillType, TextRenderingMode};
use pdf_text_engine::text::{PdfTextItem, PdfTextRun};

impl<B: CanvasBackend> TextPositioningOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn move_text_position(&mut self, tx: f32, ty: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?
            .text_state
            .move_line_position(tx, ty);
        Ok(())
    }

    fn move_text_position_and_set_leading(
        &mut self,
        tx: f32,
        ty: f32,
    ) -> Result<(), Self::ErrorType> {
        // TD: Set leading to -ty, then perform Td(tx, ty)
        let neg_ty = -ty;
        self.current_state_mut()?.text_state.leading = neg_ty;
        self.move_text_position(tx, ty)
    }

    fn set_text_matrix(&mut self, transform: &Transform) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?
            .text_state
            .set_matrices(*transform);
        Ok(())
    }

    fn move_to_start_of_next_line(&mut self) -> Result<(), Self::ErrorType> {
        // T*: Move to start of next line using current leading: Tlm = Tlm * T(0, -Tl); Tm = Tlm.
        self.current_state_mut()?.text_state.move_to_next_line();
        Ok(())
    }
}

impl<B: CanvasBackend> TextObjectOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn begin_text_object(&mut self) -> Result<(), Self::ErrorType> {
        let state = self.current_state_mut()?;
        state.text_state.matrix = Transform::identity();
        state.text_state.line_matrix = Transform::identity();
        // Clear any stale clip accumulator (guards against a missing ET from a prior object).
        state.pending_text_clip = None;
        Ok(())
    }

    fn end_text_object(&mut self) -> Result<(), Self::ErrorType> {
        // Apply any glyph outlines accumulated for clip-mode text rendering (modes 4–7).
        // Per ISO 32000 §9.3.6, the clip path is set at the end of the text object.
        if let Some(clip_path) = self.current_state_mut()?.pending_text_clip.take() {
            self.canvas
                .set_clip_region(&clip_path, pdf_graphics::PathFillType::Winding)?;
            self.current_state_mut()?.clip_path = Some(clip_path);
        }
        Ok(())
    }
}

impl<B: CanvasBackend> TextStateOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn set_character_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.style.character_spacing = spacing;
        Ok(())
    }

    fn set_word_spacing(&mut self, spacing: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.style.word_spacing = spacing;
        Ok(())
    }

    fn set_horizontal_text_scaling(&mut self, scale_percent: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.style.horizontal_scale = scale_percent / 100.0;
        Ok(())
    }

    fn set_text_leading(&mut self, leading: f32) -> Result<(), Self::ErrorType> {
        // TL sets the text leading parameter
        self.current_state_mut()?.text_state.leading = leading;
        Ok(())
    }

    fn set_font_and_size(&mut self, font_name: &[u8], size: f32) -> Result<(), Self::ErrorType> {
        let (font, nested_resources) = self
            .current_state()?
            .resources
            .as_ref()
            .and_then(|resources| resources.font(font_name))
            .ok_or_else(|| {
                PdfCanvasError::FontNotFound(String::from_utf8_lossy(font_name).into_owned())
            })?;
        let handle = self.load_pdf_font(&font)?;
        let text_state = &mut self.current_state_mut()?.text_state;
        text_state.font = Some(handle);
        text_state.font_spec = Some(font);
        text_state.resources = nested_resources;
        text_state.style.font_size = size;
        Ok(())
    }

    fn set_text_rendering_mode(&mut self, mode: TextRenderingMode) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.paint.rendering_mode = mode;
        Ok(())
    }

    fn set_text_rise(&mut self, rise: f32) -> Result<(), Self::ErrorType> {
        self.current_state_mut()?.text_state.style.rise = rise;
        Ok(())
    }
}

impl<B: CanvasBackend> TextShowingOps for PdfCanvas<'_, B> {
    type ErrorType = PdfCanvasError;
    fn show_text(&mut self, text: &PdfTextItem) -> Result<(), Self::ErrorType> {
        self.render_text_items(std::slice::from_ref(text))
    }

    fn show_text_with_glyph_positioning(
        &mut self,
        elements: &[PdfTextItem],
    ) -> Result<(), Self::ErrorType> {
        self.render_text_items(elements)
    }

    fn move_to_next_line_and_show_text(
        &mut self,
        text: &PdfTextItem,
    ) -> Result<(), Self::ErrorType> {
        self.move_to_start_of_next_line()?;
        self.show_text(text)
    }

    fn set_spacing_and_show_text(
        &mut self,
        word_spacing: f32,
        char_spacing: f32,
        text: &PdfTextItem,
    ) -> Result<(), Self::ErrorType> {
        self.set_word_spacing(word_spacing)?;
        self.set_character_spacing(char_spacing)?;
        self.move_to_next_line_and_show_text(text)
    }
}

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Paints or accumulates one scalable glyph outline.
    ///
    /// `outline` is expressed in font design coordinates. `transform` maps those coordinates into
    /// the backend's device space. The backend must apply the fill, stroke, visibility, and
    /// clipping behavior selected by [`CanvasPaint::rendering_mode`]. For a clip-capable mode,
    /// the transformed outline is accumulated for the next [`Self::commit_text_clip`] call rather
    /// than immediately changing the active clip.
    ///
    /// # Errors
    ///
    /// Returns the backend error when transforming, painting, or accumulating the outline fails.
    fn paint_outline(
        &mut self,
        path: &std::sync::Arc<PdfPath>,
        transform: &Transform,
    ) -> Result<(), PdfCanvasError> {
        match self.current_state()?.paint.rendering_mode {
            TextRenderingMode::Fill => {
                self.draw_transformed_path(path, transform, PaintMode::Fill, PathFillType::Winding)
            }
            TextRenderingMode::Stroke => self.draw_transformed_path(
                path,
                transform,
                PaintMode::Stroke,
                PathFillType::Winding,
            ),
            TextRenderingMode::FillAndStroke => self.draw_transformed_path(
                path,
                transform,
                PaintMode::FillAndStroke,
                PathFillType::Winding,
            ),
            TextRenderingMode::Invisible => Ok(()),
            TextRenderingMode::FillAndClip => {
                self.draw_transformed_path(
                    path,
                    transform,
                    PaintMode::Fill,
                    PathFillType::Winding,
                )?;
                self.add_transformed_text_clip(path, transform)
            }
            TextRenderingMode::StrokeAndClip => {
                self.draw_transformed_path(
                    path,
                    transform,
                    PaintMode::Stroke,
                    PathFillType::Winding,
                )?;
                self.add_transformed_text_clip(path, transform)
            }
            TextRenderingMode::FillStrokeAndClip => {
                self.draw_transformed_path(
                    path,
                    transform,
                    PaintMode::FillAndStroke,
                    PathFillType::Winding,
                )?;
                self.add_transformed_text_clip(path, transform)
            }
            TextRenderingMode::Clip => self.add_transformed_text_clip(path, transform),
        }
    }

    fn add_transformed_text_clip(
        &mut self,
        path: &std::sync::Arc<PdfPath>,
        transform: &Transform,
    ) -> Result<(), PdfCanvasError> {
        let mut transformed = path.as_ref().clone();
        transformed.transform(transform);
        self.add_to_text_clip(&transformed)
    }

    /// Executes one opaque PDF Type 3 character procedure.
    ///
    /// `glyph` is resolved by the PDF integration layer to the original character content stream
    /// and its nested resources. The backend must execute that content under `transform` and honor
    /// the requested visibility and clipping mode. Any temporary clip produced by the procedure
    /// remains pending until [`Self::commit_text_clip`].
    ///
    /// This method is the dependency boundary that prevents the text engine from depending on PDF
    /// content-stream parsing or a concrete canvas implementation.
    ///
    /// # Errors
    ///
    /// Returns the backend error when the handle cannot be resolved or the Type 3 content stream
    /// cannot be rendered or accumulated.
    fn paint_type3(
        &mut self,
        glyph: pdf_font::GlyphId,
        transform: &Transform,
    ) -> Result<(), PdfCanvasError> {
        let state = self.current_state()?;
        let font = state
            .text_state
            .font_spec
            .clone()
            .ok_or(PdfCanvasError::CurrentFontRequired)?;
        let resources = state.text_state.resources.clone();

        // Skipping a missing Type 3 procedure is a no-op.
        let Some(procedure) = font.type3_procedure(glyph) else {
            return Ok(());
        };

        let mut filter = |operator: &PdfOperatorVariant| {
            matches!(
                operator,
                PdfOperatorVariant::SetCharWidth(_)
                    | PdfOperatorVariant::SetCharWidthAndBoundingBox(_)
            )
        };

        // The text engine supplies a complete glyph-to-device transform. A
        // Type 3 procedure is executed through the regular content-stream
        // machinery, which normally post-concatenates its matrix onto the
        // current CTM. Temporarily use identity as that base so the page CTM
        // already present in `transform` is not applied a second time.
        self.save()?;
        let state = self.current_state_mut()?;
        state.transform = Transform::identity();
        let result = self.render_content_stream(
            procedure,
            Some(*transform),
            None,
            resources,
            Some(&mut filter),
        );
        self.restore();
        result
    }

    fn render_text_items(&mut self, items: &[PdfTextItem]) -> Result<(), PdfCanvasError> {
        let state = self.current_state()?;
        let font = state
            .text_state
            .font
            .as_ref()
            .ok_or(PdfCanvasError::CurrentFontRequired)?
            .clone();

        let style = state.text_state.style.clone();
        let rendering_mode = state.paint.rendering_mode;
        let mut base_transform = state.text_state.matrix;
        base_transform.concat(&state.transform);
        let font_system = std::sync::Arc::clone(&self.font_system);
        let type3 = font.spec().is_type3();
        let advance = font_system.visit_pdf(
            &PdfTextRun {
                font: &font,
                items,
                style,
            },
            |face, glyph| {
                let mut glyph_transform = glyph.local_transform;
                glyph_transform.concat(&base_transform);
                if rendering_mode != TextRenderingMode::Invisible {
                    if type3 {
                        self.paint_type3(glyph.glyph_id, &glyph_transform)?;
                    } else {
                        let pixels_per_em = glyph
                            .local_transform
                            .sx
                            .abs()
                            .max(glyph.local_transform.sy.abs());
                        let outline =
                            self.glyph_outline(face.as_ref(), glyph.glyph_id, pixels_per_em)?;
                        self.paint_outline(&outline, &glyph_transform)?;
                    }
                }
                if let Some(recorded) = &mut self.text_glyphs {
                    let bounds = base_transform.map_rect(&glyph.bounds).normalized();
                    if bounds.is_valid() {
                        recorded.push(crate::text::TextGlyph {
                            unicode: glyph.unicode,
                            bounds,
                        });
                    }
                }
                Ok::<(), PdfCanvasError>(())
            },
        )?;
        self.current_state_mut()?
            .text_state
            .advance(advance.x, advance.y);
        Ok(())
    }
}
