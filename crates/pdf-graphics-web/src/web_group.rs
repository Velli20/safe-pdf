//! Group rendering: content is painted on a full backing-size surface, then composited
//! onto its parent as one object.

use crate::{
    error::WebResult,
    surface::Surface,
    web_canvas_backend::{SavedContext, WebCanvasBackend},
    web_mask::composite_surface,
    web_paint,
};
use pdf_canvas::{error::PdfCanvasError, mask_layer::MaskLayer};
use pdf_graphics::{BlendMode, transform::Transform};

/// A group's isolated child canvas, owned as one rendering operation.
///
/// Construction installs inherited clips before the callback can run. Consuming
/// paint() composites only successful, balanced content, then resets and recycles
/// the surface even when the callback returns an error.
pub(crate) struct IsolatedGroup<'a> {
    parent: &'a WebCanvasBackend,
    backend: WebCanvasBackend,
    content: Surface,
}

impl<'a> IsolatedGroup<'a> {
    pub(crate) fn new(parent: &'a WebCanvasBackend) -> WebResult<Self> {
        let content = parent.surfaces.acquire(parent.viewport.backing_size())?;
        let backend = parent.isolated_on(&content)?;
        Ok(Self {
            parent,
            backend,
            content,
        })
    }

    /// Paints the group, then composites it with `alpha` and `blend_mode`.
    pub(crate) fn paint<F>(
        self,
        alpha: f32,
        blend_mode: Option<&BlendMode>,
        paint: F,
    ) -> Result<(), PdfCanvasError>
    where
        F: FnOnce(&mut WebCanvasBackend) -> Result<(), PdfCanvasError>,
    {
        let Self {
            parent,
            backend,
            content,
        } = self;
        let result = {
            let mut backend = backend;
            paint(&mut backend).and_then(|()| backend.finish_page().map_err(Into::into))
        };
        let result = result.and_then(|()| {
            composite_surface(
                &parent.context,
                content.canvas(),
                alpha,
                web_paint::blend(blend_mode),
            )
            .map_err(Into::into)
        });
        // The child backend is gone before the canvas is reset for reuse.
        parent.surfaces.recycle(content);
        result
    }
}

/// A non-isolated group's child canvas, starting from a copy of its parent's pixels.
///
/// Construction copies the backdrop, then installs inherited clips and the mask bounds
/// before the callback can run. Consuming paint() replaces the parent with an
/// interpolation between the backdrop and the content, weighted by the alpha constant
/// and the mask coverage, so the backdrop is unchanged wherever nothing was painted.
pub(crate) struct BackdropGroup<'a> {
    parent: &'a WebCanvasBackend,
    backend: WebCanvasBackend,
    content: Surface,
    coverage: Option<Surface>,
}

impl<'a> BackdropGroup<'a> {
    pub(crate) fn new(
        parent: &'a WebCanvasBackend,
        mask: Option<&MaskLayer>,
    ) -> Result<Self, PdfCanvasError> {
        let coverage = mask.map(|mask| parent.mask_coverage(mask)).transpose()?;
        let content = parent.surfaces.acquire(parent.viewport.backing_size())?;
        let backend =
            composite_surface(content.context(), &parent.canvas, 1.0, "copy").and_then(|()| {
                match mask {
                    Some(mask) => parent.isolated_within_mask(&content, mask),
                    None => parent.isolated_on(&content),
                }
            });
        match backend {
            Ok(backend) => Ok(Self {
                parent,
                backend,
                content,
                coverage,
            }),
            Err(error) => {
                parent.surfaces.recycle(content);
                if let Some(coverage) = coverage {
                    parent.surfaces.recycle(coverage);
                }
                Err(error.into())
            }
        }
    }

    /// Paints the group, then interpolates the parent towards it by `alpha` and the
    /// mask coverage.
    pub(crate) fn paint<F>(self, alpha: f32, paint: F) -> Result<(), PdfCanvasError>
    where
        F: FnOnce(&mut WebCanvasBackend) -> Result<(), PdfCanvasError>,
    {
        let Self {
            parent,
            backend,
            content,
            coverage,
        } = self;
        let result = {
            let mut backend = backend;
            paint(&mut backend).and_then(|()| backend.finish_surface().map_err(Into::into))
        };
        let result = result.and_then(|()| {
            interpolate(parent, &content, coverage.as_ref(), alpha).map_err(Into::into)
        });
        // The child backend is gone before either canvas is reset for reuse.
        parent.surfaces.recycle(content);
        if let Some(coverage) = coverage {
            parent.surfaces.recycle(coverage);
        }
        result
    }
}

/// Replaces the parent's pixels `B` with `k * content + (1 - k) * B`, where `k` is
/// `alpha` times the coverage, or `alpha` alone without a mask.
fn interpolate(
    parent: &WebCanvasBackend,
    content: &Surface,
    coverage: Option<&Surface>,
    alpha: f32,
) -> WebResult<()> {
    match coverage {
        Some(coverage) => {
            composite_surface(content.context(), coverage.canvas(), 1.0, "destination-in")?;
            composite_surface(&parent.context, coverage.canvas(), alpha, "destination-out")?;
        }
        None => {
            let context = &parent.context;
            let _saved = SavedContext::new(context);
            web_paint::set_transform(context, &Transform::identity())?;
            context.set_global_alpha(f64::from(alpha.clamp(0.0, 1.0)));
            context.set_global_composite_operation("destination-out")?;
            context.set_fill_style_str("#000");
            let size = content.size();
            context.fill_rect(0.0, 0.0, f64::from(size.width), f64::from(size.height));
        }
    }
    composite_surface(&parent.context, content.canvas(), alpha, "lighter")
}
