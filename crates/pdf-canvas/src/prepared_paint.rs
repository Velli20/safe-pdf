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
}

/// How a prepared fill paints.
pub(crate) struct FillPaint {
    /// The fill color, used directly or as the shader's alpha.
    pub(crate) color: Color,
    /// The blend mode, or `None` for normal compositing.
    pub(crate) blend_mode: Option<BlendMode>,
    /// The pattern shader, or `None` to paint the color.
    pub(crate) shader: Option<Shader>,
    /// The device-space clip of a shading pattern's `/BBox`.
    pub(crate) clip: Option<CanvasPath<'static>>,
}

/// How a prepared stroke paints.
pub(crate) struct StrokePaint {
    /// The stroke color, used directly or as the shader's alpha.
    pub(crate) color: Color,
    /// The blend mode, or `None` for normal compositing.
    pub(crate) blend_mode: Option<BlendMode>,
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
    /// A single fill-and-stroke operation paints like a knockout group: where the stroke
    /// is translucent or blended, it replaces the fill beneath it and composites with the
    /// backdrop alone, so such a pair is painted in one layer with the fill erased under
    /// the stroke.
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
            (Some(fill), Some(stroke)) if stroke.knocks_out() => {
                backend.with_group_layer(1.0, stroke.blend_mode, |backend| {
                    fill.paint(backend, path, fill_type, None)?;
                    stroke.paint_knockout(backend, path)
                })
            }
            (fill, stroke) => {
                if let Some(fill) = fill {
                    fill.paint(backend, path, fill_type, fill.blend_mode)?;
                }
                if let Some(stroke) = stroke {
                    stroke.paint(backend, path, stroke.blend_mode)?;
                }
                Ok(())
            }
        }
    }
}

impl FillPaint {
    /// Fills `path` on `backend` with `blend_mode` in place of the fill's own.
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
    /// Returns whether the stroke lets what is beneath it show through or blends with it,
    /// so that painting it over its own fill would differ from painting it over the backdrop.
    fn knocks_out(&self) -> bool {
        self.color.a < 1.0
            || self
                .blend_mode
                .is_some_and(|mode| mode != BlendMode::Normal)
    }

    /// Strokes `path` on `backend` with `blend_mode` in place of the stroke's own.
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
            self.stroke(backend, path, blend_mode)
        })
    }

    /// Erases what lies under the stroke of `path`, then strokes it with normal blending.
    ///
    /// # Errors
    /// Propagates backend failures.
    fn paint_knockout<B: CanvasBackend + ?Sized>(
        &self,
        backend: &mut B,
        path: &CanvasPath<'_>,
    ) -> Result<(), PdfCanvasError> {
        with_clip(backend, self.clip.as_ref(), |backend| {
            backend.erase_stroke(path, self.line_width, &self.style)?;
            self.stroke(backend, path, None)
        })
    }

    /// Issues the backend stroke of `path` with `blend_mode`.
    ///
    /// # Errors
    /// Propagates backend failures.
    fn stroke<B: CanvasBackend + ?Sized>(
        &self,
        backend: &mut B,
        path: &CanvasPath<'_>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        backend.stroke_path(
            path,
            self.color,
            self.line_width,
            &self.style,
            self.shader.as_ref(),
            blend_mode,
        )
    }
}
