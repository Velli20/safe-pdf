//! Isolated mask rendering: coverage and content are full backing-size surfaces.

use crate::{
    budget::{BudgetedImage, Reservation},
    canvas_state::Clip,
    error::{WebCanvasBackendError as Error, WebResult},
    surface::Surface,
    surface_pool::byte_size,
    web_canvas_backend::{SavedContext, WebCanvasBackend},
    web_paint, web_path,
};
use pdf_canvas::{
    CanvasPath,
    canvas_backend::CanvasBackend,
    error::PdfCanvasError,
    mask_layer::{CoverageTarget, MaskLayer},
};
use pdf_graphics::{Image, pdf_path::PdfPath, rect::Rect, transform::Transform};
use web_sys::CanvasRenderingContext2d;

/// A mask's coverage and isolated child canvas, owned as one rendering operation.
///
/// Construction renders coverage and installs inherited clips before the callback
/// can run. Consuming paint() commits only successful, balanced content, then
/// resets and recycles both surfaces even when the callback returns an error.
pub(crate) struct IsolatedMask<'a> {
    parent: &'a WebCanvasBackend,
    backend: WebCanvasBackend,
    content: Surface,
    coverage: Surface,
}

impl<'a> IsolatedMask<'a> {
    pub(crate) fn new(parent: &'a WebCanvasBackend, mask: &MaskLayer) -> WebResult<Self> {
        let coverage = mask.render_coverage(&mut MaskTarget {
            parent,
            _retained: None,
        })?;
        let content = parent.surfaces.acquire(parent.viewport.backing_size())?;
        let backend = Self::content_backend(parent, &content, mask)?;
        Ok(Self {
            parent,
            backend,
            content,
            coverage,
        })
    }

    pub(crate) fn paint<F>(self, paint: F) -> Result<(), PdfCanvasError>
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
            paint(&mut backend).and_then(|()| backend.finish_page().map_err(Into::into))
        };
        let result = result.and_then(|()| {
            composite_surface(content.context(), &coverage, "destination-in")?;
            composite_surface(&parent.context, &content, "source-over").map_err(Into::into)
        });
        // The child backend is gone before either canvas is reset for reuse.
        parent.surfaces.recycle(content);
        parent.surfaces.recycle(coverage);
        result
    }

    fn content_backend(
        parent: &WebCanvasBackend,
        content: &Surface,
        mask: &MaskLayer,
    ) -> WebResult<WebCanvasBackend> {
        let mut backend = WebCanvasBackend::on_surface(
            content,
            parent.viewport.device_size(),
            *parent.viewport.device_to_backing(),
            parent.surfaces.clone(),
        )?;
        for clip in &parent.state.clips {
            backend
                .context
                .clip_with_path_2d_and_winding(&clip.path, clip.rule);
        }
        backend.state.clips.clone_from(&parent.state.clips);
        let bounds = PdfPath::from(&Rect::new(
            mask.recording().width(),
            mask.recording().height(),
        ));
        let bounds = web_path::prepare(&CanvasPath::transformed(&bounds, *mask.transform())?)?;
        backend.context.clip_with_path_2d(&bounds);
        // Retain the mask bounds when nested masks copy their inherited clipping.
        backend.state.clips.push(Clip {
            path: bounds,
            rule: web_sys::CanvasWindingRule::Nonzero,
        });
        Ok(backend)
    }
}

/// Renders coverage in backing space with the mask transform baked in.
///
/// Readback pixels and processing scratch stay charged until the processed
/// coverage has been uploaded.
struct MaskTarget<'a> {
    parent: &'a WebCanvasBackend,
    _retained: Option<(BudgetedImage, Reservation)>,
}

impl CoverageTarget for MaskTarget<'_> {
    type Raster = Surface;

    fn replay_mask(&mut self, mask: &MaskLayer) -> Result<Surface, PdfCanvasError> {
        Ok(self.parent.replay_surface(
            mask.recording(),
            self.parent.viewport.backing_size(),
            self.parent
                .viewport
                .device_to_backing()
                .post_concatenated(mask.transform()),
        )?)
    }

    fn read(&mut self, raster: Surface) -> Result<Image, PdfCanvasError> {
        let pool = &self.parent.surfaces;
        let image = pool.read(&raster)?;
        // Luminosity conversion and transfer may each allocate a coverage buffer.
        let scratch = pool.scratch(
            byte_size(raster.size())?
                .checked_mul(2)
                .ok_or(Error::ResourceLimit)?,
        )?;
        pool.recycle(raster);
        let pixels = image.image().clone();
        self._retained = Some((image, scratch));
        Ok(pixels)
    }

    fn upload(&mut self, image: &Image) -> Result<Surface, PdfCanvasError> {
        Ok(self.parent.surfaces.upload(image)?)
    }
}

/// Composites a full backing-size surface onto a context using a native operation.
///
/// Resetting the transform prevents the device-to-backing mapping from being
/// applied a second time.
fn composite_surface(
    context: &CanvasRenderingContext2d,
    source: &Surface,
    operation: &str,
) -> WebResult<()> {
    let _saved = SavedContext::new(context);
    web_paint::set_transform(context, &Transform::identity())?;
    context.set_global_alpha(1.0);
    context.set_global_composite_operation(operation)?;
    context.draw_image_with_html_canvas_element(source.canvas(), 0.0, 0.0)?;
    Ok(())
}
