//! Image placement in logical device coordinates with budgeted pixel upload.

use crate::{
    error::{WebCanvasBackendError as Error, WebResult},
    surface::Surface,
    surface_pool::{SurfacePool, upload_bytes},
    web_canvas_backend::{SavedContext, WebCanvasBackend},
    web_paint,
};
use num_traits::ToPrimitive;
use pdf_graphics::{BlendMode, Image, PixelFormat, size::Size, transform::Transform};
use pdf_image::ReduceImage;
use web_sys::CanvasRenderingContext2d;

/// Validated image placement for one target.
///
/// The uploaded surface is owned until drawing finishes, so its reservation
/// cannot be released while browser compositing still needs it.
pub(crate) struct ImageDraw<'a> {
    context: &'a CanvasRenderingContext2d,
    pool: &'a SurfacePool,
    image: &'a Image,
    reduction: Reduction,
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
            reduction: Reduction::new(image, &unit_to_backing, &backend.surfaces)?,
            placement: ImagePlacement::new(transform)?,
            smoothing: !image.replicates_pixels(&unit_to_backing),
            composite: web_paint::blend(mode.as_ref()),
            alpha,
        })
    }

    pub(crate) fn draw(self) -> WebResult<()> {
        let surface = match self.reduction.factor {
            1 => self.pool.upload(self.image)?,
            factor => {
                // The reduced copy stays charged until its pixels have been uploaded.
                let _reduced = self
                    .pool
                    .scratch(self.reduction.reduced_bytes(self.image)?)?;
                self.pool.upload(&self.image.reduced(factor)?)?
            }
        };
        let _saved = SavedContext::new(self.context);
        self.context.set_global_alpha(f64::from(self.alpha));
        self.context
            .set_global_composite_operation(self.composite)?;
        self.context.set_image_smoothing_enabled(self.smoothing);
        self.placement.draw(self.context, &surface)
    }
}

/// The whole factor an image is box-reduced by before upload.
///
/// Uploading more pixels than the backing pixels an image covers only costs
/// storage, and Canvas downsamples without averaging, so a reduced upload is
/// both smaller and smoother. The factor never reduces a side below the backing
/// pixels it spans, unless that is needed for the upload to fit the budget.
#[derive(Clone, Copy)]
struct Reduction {
    factor: usize,
}

impl Reduction {
    fn new(image: &Image, unit_to_backing: &Transform, pool: &SurfacePool) -> WebResult<Self> {
        let width = f64::from(unit_to_backing.sx).hypot(f64::from(unit_to_backing.ky));
        let height = f64::from(unit_to_backing.kx).hypot(f64::from(unit_to_backing.sy));
        let mut reduction = Self {
            factor: covering_factor(image.width, width).min(covering_factor(image.height, height)),
        };
        let longest = image.width.max(image.height);
        while reduction.peak_bytes(image)? > pool.limit() {
            if reduction.factor >= longest {
                return Err(Error::ResourceLimit);
            }
            reduction.factor = reduction
                .factor
                .checked_add(1)
                .ok_or(Error::ResourceLimit)?;
        }
        Ok(reduction)
    }

    /// Returns the dimensions of the image once reduced.
    fn size(&self, image: &Image) -> WebResult<Size<u32>> {
        let side = |pixels: usize| {
            u32::try_from(pixels.div_ceil(self.factor)).map_err(|_| Error::ResourceLimit)
        };
        Ok(Size::new(side(image.width)?, side(image.height)?))
    }

    /// Returns the bytes of the reduced copy, in the image's own pixel format.
    fn reduced_bytes(&self, image: &Image) -> WebResult<usize> {
        let Size { width, height } = self.size(image)?;
        let bytes_per_pixel = match image.pixel_format {
            PixelFormat::RGBA8888 => 4,
            PixelFormat::Gray8 | PixelFormat::Alpha8 => 1,
        };
        usize::try_from(width)
            .ok()
            .zip(usize::try_from(height).ok())
            .and_then(|(width, height)| width.checked_mul(height))
            .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
            .ok_or(Error::ResourceLimit)
    }

    /// Returns the accounted bytes drawing holds at its peak: the upload, and the
    /// reduced copy it is made from.
    fn peak_bytes(&self, image: &Image) -> WebResult<usize> {
        let upload = upload_bytes(self.size(image)?, image.pixel_format)?;
        match self.factor {
            1 => Ok(upload),
            _ => upload
                .checked_add(self.reduced_bytes(image)?)
                .ok_or(Error::ResourceLimit),
        }
    }
}

/// Returns the largest whole factor that leaves `pixels` source pixels at least as
/// many as the `extent` backing pixels they span, and at least one.
fn covering_factor(pixels: usize, extent: f64) -> usize {
    pixels
        .to_f64()
        .and_then(|pixels| (pixels / extent.max(1.0)).floor().to_usize())
        .unwrap_or(1)
        .max(1)
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
