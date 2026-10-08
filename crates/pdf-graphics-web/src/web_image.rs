//! Image placement in logical device coordinates with budgeted pixel upload.

use crate::{
    error::{WebCanvasBackendError as Error, WebResult},
    surface::Surface,
    surface_pool::SurfacePool,
    web_canvas_backend::{SavedContext, WebCanvasBackend},
    web_paint,
};
use pdf_graphics::{BlendMode, Image, transform::Transform};
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
    smoothing: bool,
    composite: &'static str,
    alpha: f32,
}

impl<'a> ImageDraw<'a> {
    pub(crate) fn new(
        backend: &'a WebCanvasBackend,
        image: &'a Image,
        mode: Option<BlendMode>,
        alpha: f32,
        transform: Transform,
    ) -> WebResult<Self> {
        let unit_to_backing = backend
            .viewport
            .device_to_backing()
            .post_concatenated(&transform);
        Ok(Self {
            context: &backend.context,
            pool: &backend.surfaces,
            image,
            placement: ImagePlacement::new(transform)?,
            smoothing: !image.replicates_pixels(&unit_to_backing),
            composite: web_paint::blend(mode.as_ref()),
            alpha,
        })
    }

    pub(crate) fn draw(self) -> WebResult<()> {
        let surface = self.pool.upload(self.image)?;
        let _saved = SavedContext::new(self.context);
        self.context.set_global_alpha(f64::from(self.alpha));
        self.context
            .set_global_composite_operation(self.composite)?;
        self.context.set_image_smoothing_enabled(self.smoothing);
        self.placement.draw(self.context, &surface)
    }
}

/// A finite mapping from the image's unit square to logical device space.
struct ImagePlacement {
    transform: Transform,
}

impl ImagePlacement {
    fn new(transform: Transform) -> WebResult<Self> {
        transform
            .validate()
            .map_err(|_| Error::InvalidInput("image placement"))?;
        Ok(Self { transform })
    }

    fn draw(&self, context: &CanvasRenderingContext2d, surface: &Surface) -> WebResult<()> {
        // Applied after the existing device-to-backing mapping, so the unit square
        // lands where the transform places it in logical device space.
        let [a, b, c, d, e, f] = self.transform.to_row().map(f64::from);
        context.transform(a, b, c, d, e, f)?;
        context.draw_image_with_html_canvas_element_and_dw_and_dh(
            surface.canvas(),
            0.0,
            0.0,
            1.0,
            1.0,
        )?;
        Ok(())
    }
}
