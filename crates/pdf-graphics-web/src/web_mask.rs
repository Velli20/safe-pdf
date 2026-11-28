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
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

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
        let coverage = parent.mask_coverage(mask)?;
        let content = parent.surfaces.acquire(parent.viewport.backing_size())?;
        let backend = parent.isolated_within_mask(&content, mask)?;
        Ok(Self {
            parent,
            backend,
            content,
            coverage,
        })
    }

    /// Paints the content, then composites it onto the parent through the coverage.
    ///
    /// The coverage applies without the content's clips: their anti-aliased edges would
    /// apply it only in part and leave unmasked content behind.
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
            paint(&mut backend).and_then(|()| backend.finish_surface().map_err(Into::into))
        };
        let result = result.and_then(|()| {
            composite_surface(content.context(), coverage.canvas(), 1.0, "destination-in")?;
            composite_surface(&parent.context, content.canvas(), 1.0, "source-over")
                .map_err(Into::into)
        });
        // The child backend is gone before either canvas is reset for reuse.
        parent.surfaces.recycle(content);
        parent.surfaces.recycle(coverage);
        result
    }
}

impl WebCanvasBackend {
    /// Renders the coverage of `mask` onto a full backing-size surface.
    pub(crate) fn mask_coverage(&self, mask: &MaskLayer) -> Result<Surface, PdfCanvasError> {
        mask.render_coverage(&mut MaskTarget {
            parent: self,
            _retained: None,
        })
    }

    /// Creates a child backend on `content` with this backend's clips and the bounds of
    /// `mask`, outside which masked content cannot show.
    pub(crate) fn isolated_within_mask(
        &self,
        content: &Surface,
        mask: &MaskLayer,
    ) -> WebResult<WebCanvasBackend> {
        let mut backend = self.isolated_on(content)?;
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

/// Composites a full backing-size canvas onto a context with a constant `alpha` and a
/// native operation.
///
/// Resetting the transform prevents the device-to-backing mapping from being
/// applied a second time.
pub(crate) fn composite_surface(
    context: &CanvasRenderingContext2d,
    source: &HtmlCanvasElement,
    alpha: f32,
    operation: &str,
) -> WebResult<()> {
    let _saved = SavedContext::new(context);
    web_paint::set_transform(context, &Transform::identity())?;
    context.set_global_alpha(f64::from(alpha.clamp(0.0, 1.0)));
    context.set_global_composite_operation(operation)?;
    context.draw_image_with_html_canvas_element(source, 0.0, 0.0)?;
    Ok(())
}
