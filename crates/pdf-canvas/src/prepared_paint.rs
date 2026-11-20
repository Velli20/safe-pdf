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
    /// # Errors
    /// Propagates backend failures.
    pub(crate) fn paint<B: CanvasBackend + ?Sized>(
        &self,
        backend: &mut B,
        path: &CanvasPath<'_>,
        fill_type: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        if let Some(fill) = &self.fill {
            with_clip(backend, fill.clip.as_ref(), |backend| {
                backend.fill_path(
                    path,
                    fill_type,
                    fill.color,
                    fill.shader.as_ref(),
                    fill.blend_mode,
                )
            })?;
        }
        if let Some(stroke) = &self.stroke {
            with_clip(backend, stroke.clip.as_ref(), |backend| {
                backend.stroke_path(
                    path,
                    stroke.color,
                    stroke.line_width,
                    &stroke.style,
                    stroke.shader.as_ref(),
                    stroke.blend_mode,
                )
            })?;
        }
        Ok(())
    }
}
