//! Image placement in logical device coordinates with budgeted pixel upload.

use crate::{
    error::{WebCanvasBackendError as Error, WebResult},
    surface::Surface,
    surface_pool::SurfacePool,
    web_canvas_backend::{SavedContext, WebCanvasBackend},
    web_paint,
};
use pdf_graphics::{BlendMode, Image, rect::Rect};
use web_sys::CanvasRenderingContext2d;

/// Validated image placement for one target.
///
/// The uploaded surface is owned until drawing finishes, so its reservation
/// cannot be released while browser compositing still needs it.
pub(crate) struct ImageDraw<'a> {
    context: &'a CanvasRenderingContext2d,
    pool: &'a SurfacePool,
    image: &'a Image,
    placement: ImagePlacement,
    composite: &'static str,
}

impl<'a> ImageDraw<'a> {
    pub(crate) fn new(
        backend: &'a WebCanvasBackend,
        image: &'a Image,
        mode: Option<BlendMode>,
        rect: Rect,
        rotation: Option<f32>,
    ) -> WebResult<Self> {
        Ok(Self {
            context: &backend.context,
            pool: &backend.surfaces,
            image,
            placement: ImagePlacement::new(rect, rotation)?,
            composite: web_paint::blend(mode.as_ref()),
        })
    }

    pub(crate) fn draw(self) -> WebResult<()> {
        let surface = self.pool.upload(self.image)?;
        let _saved = SavedContext::new(self.context);
        self.context.set_global_alpha(1.0);
        self.context
            .set_global_composite_operation(self.composite)?;
        self.context.set_image_smoothing_enabled(true);
        self.placement.draw(self.context, &surface)
    }
}

/// A valid logical-device rectangle and optional finite rotation in radians.
struct ImagePlacement {
    rect: Rect,
    rotation: Option<f64>,
}

impl ImagePlacement {
    fn new(rect: Rect, rotation: Option<f32>) -> WebResult<Self> {
        if !rect.is_valid() || rotation.is_some_and(|angle| !angle.is_finite()) {
            return Err(Error::InvalidInput("image placement"));
        }
        Ok(Self {
            rect,
            rotation: rotation.map(|angle| f64::from(angle).to_radians()),
        })
    }

    fn draw(&self, context: &CanvasRenderingContext2d, surface: &Surface) -> WebResult<()> {
        if let Some(angle) = self.rotation {
            // Rotate about the placed rectangle's center before the existing
            // device-to-backing mapping is applied.
            let x = f64::from(self.rect.left) + f64::from(self.rect.width()) / 2.0;
            let y = f64::from(self.rect.top) + f64::from(self.rect.height()) / 2.0;
            context.translate(x, y)?;
            context.rotate(angle)?;
            context.translate(-x, -y)?;
        }
        context.draw_image_with_html_canvas_element_and_dw_and_dh(
            surface.canvas(),
            f64::from(self.rect.left),
            f64::from(self.rect.top),
            f64::from(self.rect.width()),
            f64::from(self.rect.height()),
        )?;
        Ok(())
    }
}
