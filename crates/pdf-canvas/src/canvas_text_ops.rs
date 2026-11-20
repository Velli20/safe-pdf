use crate::CanvasPath;
use crate::canvas_backend::CanvasBackend;
use crate::error::PdfCanvasError;
use crate::pdf_canvas::PdfCanvas;
use pdf_content_stream_operators::variants::PdfOperatorVariant;
use pdf_graphics::pdf_path::PdfPath;
use pdf_graphics::transform::Transform;
use pdf_graphics::{PaintMode, PathFillType, TextRenderingMode};
use pdf_text_engine::text::{PdfTextItem, PdfTextRun};
use std::sync::Arc;

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Handles `Td`: offsets the start of the next line.
    pub(crate) fn move_text_position(&mut self, tx: f32, ty: f32) -> Result<(), PdfCanvasError> {
        self.state.text_state.move_line_position(tx, ty);
        Ok(())
    }

    /// Handles `TD`: sets the leading to `-ty` and offsets the next line.
    pub(crate) fn move_text_position_and_set_leading(
        &mut self,
        tx: f32,
        ty: f32,
    ) -> Result<(), PdfCanvasError> {
        // TD: Set leading to -ty, then perform Td(tx, ty)
        let neg_ty = -ty;
        self.state.text_state.leading = neg_ty;
        self.move_text_position(tx, ty)
    }

    /// Handles `Tm`: sets the text and text line matrices.
    pub(crate) fn set_text_matrix(&mut self, transform: &Transform) -> Result<(), PdfCanvasError> {
        self.state.text_state.set_matrices(*transform);
        Ok(())
    }

    /// Handles `T*`: moves to the start of the next line.
    pub(crate) fn move_to_start_of_next_line(&mut self) -> Result<(), PdfCanvasError> {
        // T*: Move to start of next line using current leading: Tlm = Tlm * T(0, -Tl); Tm = Tlm.
        self.state.text_state.move_to_next_line();
        Ok(())
    }
}

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Handles `BT`: resets the text matrices.
    pub(crate) fn begin_text_object(&mut self) -> Result<(), PdfCanvasError> {
        let state = &mut self.state;
        state.text_state.matrix = Transform::identity();
        state.text_state.line_matrix = Transform::identity();
        // Clear any stale clip accumulator (guards against a missing ET from a prior object).
        self.pending_text_clip = None;
        Ok(())
    }

    /// Handles `ET`: applies any pending text clip.
    pub(crate) fn end_text_object(&mut self) -> Result<(), PdfCanvasError> {
        // Apply any glyph outlines accumulated for clip-mode text rendering (modes 4–7).
        // Per ISO 32000 §9.3.6, the clip path is set at the end of the text object.
        if let Some(clip_path) = self.pending_text_clip.take() {
            let clip_path =
                CanvasPath::shared(std::sync::Arc::new(clip_path), Transform::identity())?;
            self.canvas
                .set_clip_region(&clip_path, pdf_graphics::PathFillType::Winding)?;
            self.state.clip_path = Some(clip_path);
        }
        Ok(())
    }
}

impl<B: CanvasBackend> PdfCanvas<'_, B> {
    /// Handles `Tc`: sets the character spacing.
    pub(crate) fn set_character_spacing(&mut self, spacing: f32) -> Result<(), PdfCanvasError> {
        self.state.text_state.style.character_spacing = spacing;
        Ok(())
    }

    /// Handles `Tw`: sets the word spacing.
    pub(crate) fn set_word_spacing(&mut self, spacing: f32) -> Result<(), PdfCanvasError> {
        self.state.text_state.style.word_spacing = spacing;
        Ok(())
    }

    /// Handles `Tz`: sets the horizontal scaling from a percentage.
    pub(crate) fn set_horizontal_text_scaling(
        &mut self,
        scale_percent: f32,
    ) -> Result<(), PdfCanvasError> {
        self.state.text_state.style.horizontal_scale = scale_percent / 100.0;
        Ok(())
    }

    /// Handles `TL`: sets the text leading.
    pub(crate) fn set_text_leading(&mut self, leading: f32) -> Result<(), PdfCanvasError> {
        // TL sets the text leading parameter
        self.state.text_state.leading = leading;
        Ok(())
    }

    /// Handles `Tf`: selects a font resource and size, looking a name the current
    /// resources lack up in the fallback fonts, then substituting a Standard 14 font.
    pub(crate) fn set_font_and_size(
        &mut self,
        font_name: &[u8],
        size: f32,
    ) -> Result<(), PdfCanvasError> {
        let resources = self.state.resources.clone();
        let (handle, nested_resources) = self.load_named_font(font_name, resources.as_deref())?;
        let text_state = &mut self.state.text_state;
        text_state.font = Some(handle);
        text_state.resources = nested_resources;
        text_state.style.font_size = size;
        Ok(())
    }

    /// Handles `Tr`: sets the text rendering mode.
    pub(crate) fn set_text_rendering_mode(
        &mut self,
        mode: TextRenderingMode,
    ) -> Result<(), PdfCanvasError> {
        self.state.paint.rendering_mode = mode;
        Ok(())
    }

    /// Handles `Ts`: sets the text rise.
    pub(crate) fn set_text_rise(&mut self, rise: f32) -> Result<(), PdfCanvasError> {
        self.state.text_state.style.rise = rise;
        Ok(())
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
        let mode = self.state.paint.rendering_mode;
        if matches!(mode, TextRenderingMode::Invisible) {
            return Ok(());
        }
        let path = CanvasPath::shared(std::sync::Arc::clone(path), *transform)?;
        match mode {
            TextRenderingMode::Fill => {
                self.draw_path(&path, PaintMode::Fill, PathFillType::Winding)
            }
            TextRenderingMode::Stroke => {
                self.draw_path(&path, PaintMode::Stroke, PathFillType::Winding)
            }
            TextRenderingMode::FillAndStroke => {
                self.draw_path(&path, PaintMode::FillAndStroke, PathFillType::Winding)
            }
            TextRenderingMode::Invisible => Ok(()),
            TextRenderingMode::FillAndClip => {
                self.draw_path(&path, PaintMode::Fill, PathFillType::Winding)?;
                self.add_to_text_clip(&path)
            }
            TextRenderingMode::StrokeAndClip => {
                self.draw_path(&path, PaintMode::Stroke, PathFillType::Winding)?;
                self.add_to_text_clip(&path)
            }
            TextRenderingMode::FillStrokeAndClip => {
                self.draw_path(&path, PaintMode::FillAndStroke, PathFillType::Winding)?;
                self.add_to_text_clip(&path)
            }
            TextRenderingMode::Clip => self.add_to_text_clip(&path),
        }
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
        let state = &self.state;
        let font = state
            .text_state
            .font
            .as_ref()
            .map(|font| Arc::clone(font.shared_spec()))
            .ok_or(PdfCanvasError::CurrentFontRequired)?;
        let resources = state.text_state.resources.clone();

        // Skipping a missing Type 3 procedure is a no-op.
        let Some(procedure) = font.type3_procedure(glyph) else {
            return Ok(());
        };

        // A procedure started with `d1` describes an uncolored glyph: it is painted in the
        // current fill color, so its own color operators are ignored.
        let uncolored = matches!(
            procedure.operators.first(),
            Some(PdfOperatorVariant::SetCharWidthAndBoundingBox { .. })
        );
        let mut filter = |operator: &PdfOperatorVariant| {
            matches!(
                operator,
                PdfOperatorVariant::SetCharWidth { .. }
                    | PdfOperatorVariant::SetCharWidthAndBoundingBox { .. }
            ) || (uncolored && operator.sets_color())
        };

        // The text engine supplies a complete glyph-to-device transform. A
        // Type 3 procedure is executed through the regular content-stream
        // machinery, which normally post-concatenates its matrix onto the
        // current CTM. Temporarily use identity as that base so the page CTM
        // already present in `transform` is not applied a second time.
        self.save()?;
        let state = &mut self.state;
        state.transform = Transform::identity();
        let result = self.render_content_stream(
            procedure,
            Some(*transform),
            None,
            resources,
            Some(&mut filter),
        );
        let restore_result = self.restore();
        result.and(restore_result)
    }

    /// Handles `Tj`/`TJ`: shows text items, then advances the text matrix past them.
    pub(crate) fn render_text_items(
        &mut self,
        items: &[PdfTextItem],
    ) -> Result<(), PdfCanvasError> {
        let state = &self.state;
        let font = state
            .text_state
            .font
            .as_ref()
            .map(Arc::clone)
            .ok_or(PdfCanvasError::CurrentFontRequired)?;

        let style = state.text_state.style;
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
                        let outline = self.glyph_outline(face.as_ref(), glyph.glyph_id)?;
                        self.paint_outline(&outline, &glyph_transform)?;
                    }
                }
                let hidden = self.content_hidden();
                if let Some(recorded) = self.text_glyphs.as_mut().filter(|_| !hidden) {
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
        self.state.text_state.advance(advance.x, advance.y);
        Ok(())
    }
}
