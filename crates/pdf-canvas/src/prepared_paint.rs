use pdf_graphics::{BlendMode, PathFillType, color::Color};

use crate::{
    CanvasPath,
    canvas_backend::{CanvasBackend, Shader},
    error::PdfCanvasError,
    shading::with_clip,
    stroke_style::StrokeStyle,
};

/// The fill and stroke a path-painting operation applies, worked out from the graphics
/// state once and reusable for every path painted while that state holds, such as the
/// glyphs of one text run.
pub(crate) struct PreparedPaint {
    /// The fill, or `None` when the operation does not fill or the fill leaves no marks.
    pub(crate) fill: Option<FillPaint>,
    /// The stroke, or `None` when the operation does not stroke or the stroke leaves no marks.
    pub(crate) stroke: Option<StrokePaint>,
    /// The blend mode of both the fill and the stroke, or `None` for normal compositing.
    pub(crate) blend_mode: Option<BlendMode>,
}

/// How a prepared fill paints.
pub(crate) struct FillPaint {
    /// The fill color, used directly or as the shader's alpha.
    pub(crate) color: Color,
    /// The pattern shader, or `None` to paint the color.
    pub(crate) shader: Option<Shader>,
    /// The device-space clip of a shading pattern's `/BBox`.
    pub(crate) clip: Option<CanvasPath<'static>>,
}

/// How a prepared stroke paints.
pub(crate) struct StrokePaint {
    /// The stroke color, used directly or as the shader's alpha.
    pub(crate) color: Color,
    /// The line width in stroke space.
    pub(crate) line_width: f32,
    /// Caps, joins, dashes and the stroke-space transform.
    pub(crate) style: StrokeStyle,
    /// The pattern shader, or `None` to paint the color.
    pub(crate) shader: Option<Shader>,
    /// The device-space clip of a shading pattern's `/BBox`.
    pub(crate) clip: Option<CanvasPath<'static>>,
}

impl PreparedPaint {
    /// Fills, then strokes, `path` on `backend`.
    ///
    /// A single fill-and-stroke operation paints like a knockout group: the stroke replaces
    /// the fill beneath it and composites with the backdrop alone. That differs from
    /// stroking over the fill only when the stroke is translucent or blended, so only then
    /// is the pair painted in one layer, with the stroke knocking out the fill.
    ///
    /// # Errors
    /// Propagates backend failures.
    pub(crate) fn paint<B: CanvasBackend>(
        &self,
        backend: &mut B,
        path: &CanvasPath<'_>,
        fill_type: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        match (&self.fill, &self.stroke) {
            (Some(fill), Some(stroke)) if stroke.color.a < 1.0 || self.is_blended() => backend
                .with_group_layer(1.0, self.blend_mode, |backend| {
                    fill.paint(backend, path, fill_type, None)?;
                    stroke.paint_knockout(backend, path)
                }),
            (fill, stroke) => {
                if let Some(fill) = fill {
                    fill.paint(backend, path, fill_type, self.blend_mode)?;
                }
                if let Some(stroke) = stroke {
                    stroke.paint(backend, path, self.blend_mode)?;
                }
                Ok(())
            }
        }
    }

    /// Returns whether the paint blends with what lies beneath it instead of covering it.
    fn is_blended(&self) -> bool {
        self.blend_mode
            .is_some_and(|mode| mode != BlendMode::Normal)
    }
}

impl FillPaint {
    /// Fills `path` on `backend` with `blend_mode`.
    ///
    /// # Errors
    /// Propagates backend failures.
    fn paint<B: CanvasBackend + ?Sized>(
        &self,
        backend: &mut B,
        path: &CanvasPath<'_>,
        fill_type: PathFillType,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        with_clip(backend, self.clip.as_ref(), |backend| {
            backend.fill_path(
                path,
                fill_type,
                self.color,
                self.shader.as_ref(),
                blend_mode,
            )
        })
    }
}

impl StrokePaint {
    /// Strokes `path` on `backend` with `blend_mode`.
    ///
    /// # Errors
    /// Propagates backend failures.
    fn paint<B: CanvasBackend + ?Sized>(
        &self,
        backend: &mut B,
        path: &CanvasPath<'_>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        with_clip(backend, self.clip.as_ref(), |backend| {
            backend.stroke_path(
                path,
                self.color,
                self.line_width,
                &self.style,
                self.shader.as_ref(),
                blend_mode,
            )
        })
    }

    /// Strokes `path` on `backend`, replacing what lies under the stroke.
    ///
    /// # Errors
    /// Propagates backend failures.
    fn paint_knockout<B: CanvasBackend + ?Sized>(
        &self,
        backend: &mut B,
        path: &CanvasPath<'_>,
    ) -> Result<(), PdfCanvasError> {
        with_clip(backend, self.clip.as_ref(), |backend| {
            backend.knockout_stroke(
                path,
                self.color,
                self.line_width,
                &self.style,
                self.shader.as_ref(),
            )
        })
    }
}
