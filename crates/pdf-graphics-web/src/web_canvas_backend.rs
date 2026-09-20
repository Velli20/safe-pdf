//! Browser rendering with owned paint resources and isolated mask targets.
//!
//! The backend owns page lifecycle and the PDF save/clip stack. Prepared paths
//! own the resources needed for their operations and use logical device
//! coordinates. Shader rasters, image draws, and mask layers live in the sibling
//! `web_shader`, `web_image`, and `web_mask` modules.

use crate::{
    canvas_state::{BackendState, Clip},
    error::{WebCanvasBackendError as Error, WebResult},
    surface::{Surface, context},
    surface_pool::{SurfacePool, byte_size},
    web_image::ImageDraw,
    web_mask::IsolatedMask,
    web_paint, web_path,
    web_shader::ShaderRaster,
};
use pdf_canvas::{
    CanvasPath, CanvasViewport,
    canvas_backend::{CanvasBackend, Shader},
    error::PdfCanvasError,
    mask_layer::MaskLayer,
    recording_canvas::RecordingCanvas,
    stroke_style::{StrokeStyle, device_stroke_width},
};
use pdf_graphics::{
    BlendMode, Image, PathFillType, color::Color, rect::Rect, transform::Transform,
};
use web_sys::{CanvasPattern, CanvasRenderingContext2d, CanvasWindingRule, Path2d};

/// Memory limit for recursive temporary storage.
#[derive(Clone, Copy)]
pub struct WebCanvasOptions {
    /// Maximum aggregate accounted bytes for temporary backing stores and pixel buffers.
    /// This does not measure total browser memory or include the host target canvas.
    pub temporary_bytes: usize,
}

impl Default for WebCanvasOptions {
    fn default() -> Self {
        Self {
            temporary_bytes: 128 * 1024 * 1024,
        }
    }
}

/// Main-thread HTML Canvas 2D target. Hosts must not mutate the canvas while rendering.
///
/// Each context has a baseline save beneath the PDF save stack. Temporary paint
/// saves are restored within each draw call and never enter that stack. Mask and
/// pattern replays use separate backends sharing this backend's surface budget.
pub struct WebCanvasBackend {
    canvas: web_sys::HtmlCanvasElement,
    pub(crate) context: CanvasRenderingContext2d,
    pub(crate) viewport: CanvasViewport,
    pub(crate) state: BackendState,
    pub(crate) surfaces: SurfacePool,
}

impl WebCanvasBackend {
    /// Acquires and resets Canvas 2D using the supplied logical and backing dimensions.
    pub fn new(
        canvas: web_sys::HtmlCanvasElement,
        viewport: CanvasViewport,
        options: WebCanvasOptions,
    ) -> WebResult<Self> {
        let context = context(&canvas)?;
        let mut backend = Self {
            canvas,
            context,
            viewport,
            state: BackendState::default(),
            surfaces: SurfacePool::new(options.temporary_bytes),
        };
        backend.install(viewport)?;
        Ok(backend)
    }

    /// Returns the logical device and backing-store mapping.
    pub fn viewport(&self) -> &CanvasViewport {
        &self.viewport
    }

    /// Resizes a balanced backend, clearing pixels and temporary surfaces.
    pub fn resize(&mut self, viewport: CanvasViewport) -> WebResult<()> {
        self.state.ensure_balanced()?;
        self.surfaces.clear();
        self.install(viewport)
    }

    /// Verifies that all page scopes have been closed.
    pub fn finish_page(&mut self) -> WebResult<()> {
        self.state.ensure_balanced()
    }

    /// Unwinds scopes after a failure without undoing previously painted page pixels.
    pub fn abort_page(&mut self) -> WebResult<()> {
        // Unwind PDF saves first, then the baseline save to remove page clips.
        while self.state.saved.pop().is_some() {
            self.context.restore();
        }
        self.context.restore();
        self.reset_scopes()
    }

    /// Sizes the host canvas for the viewport, clearing its pixels, and opens fresh scopes.
    fn install(&mut self, viewport: CanvasViewport) -> WebResult<()> {
        let [w, h] = viewport.backing_size();
        byte_size([w, h])?;
        self.canvas.set_width(w);
        self.canvas.set_height(h);
        self.viewport = viewport;
        self.reset_scopes()
    }

    /// Applies the viewport mapping and takes the baseline save beneath an empty PDF stack.
    fn reset_scopes(&mut self) -> WebResult<()> {
        web_paint::set_transform(&self.context, self.viewport.device_to_backing())?;
        self.context.save();
        self.state = BackendState::default();
        Ok(())
    }

    /// Re-targets a balanced backend at another mapping of the same surface.
    pub(crate) fn set_mapping(&mut self, viewport: CanvasViewport) -> WebResult<()> {
        self.state.ensure_balanced()?;
        web_paint::set_transform(&self.context, viewport.device_to_backing())?;
        self.viewport = viewport;
        Ok(())
    }

    /// Uses a temporary surface's browser handles without taking ownership of its pixels.
    ///
    /// The caller must keep the surface alive until this backend is dropped. The
    /// surface owner controls its backing dimensions and budget reservation.
    pub(crate) fn on_surface(
        surface: &Surface,
        device_size: [f32; 2],
        mapping: Transform,
        pool: SurfacePool,
    ) -> WebResult<Self> {
        let viewport = CanvasViewport::with_transform(device_size, surface.size(), mapping)?;
        let mut backend = Self {
            canvas: surface.canvas().clone(),
            context: surface.context().clone(),
            viewport,
            state: BackendState::default(),
            surfaces: pool,
        };
        backend.reset_scopes()?;
        Ok(backend)
    }

    /// Replays a recording and balances or aborts its browser scopes.
    pub(crate) fn replay_balanced(&mut self, recording: &RecordingCanvas) -> WebResult<()> {
        self.finish_page()?;
        let result = recording
            .replay(self)
            .map_err(Error::from)
            .and_then(|()| self.finish_page());
        if result.is_err() {
            // Preserve the replay/validation error even if abort also fails.
            let _ = self.abort_page();
        }
        result
    }

    /// Allocates a temporary target and replays the recording under the supplied mapping.
    pub(crate) fn replay_surface(
        &self,
        recording: &RecordingCanvas,
        size: [u32; 2],
        mapping: Transform,
    ) -> WebResult<Surface> {
        let surface = self.surfaces.acquire(size)?;
        {
            let mut backend = Self::on_surface(
                &surface,
                [recording.width(), recording.height()],
                mapping,
                self.surfaces.clone(),
            )?;
            backend.replay_balanced(recording)?;
        }
        Ok(surface)
    }
}

impl CanvasBackend for WebCanvasBackend {
    fn width(&self) -> f32 {
        let [width, _] = self.viewport.device_size();
        width
    }

    fn height(&self) -> f32 {
        let [_, height] = self.viewport.device_size();
        height
    }

    fn fill_path(
        &mut self,
        path: &CanvasPath<'_>,
        rule: PathFillType,
        color: Color,
        shader: Option<&Shader>,
        mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        PreparedPath::new(self, path, color, shader, mode)?
            .fill(rule)
            .map_err(Into::into)
    }

    fn stroke_path(
        &mut self,
        path: &CanvasPath<'_>,
        color: Color,
        width: f32,
        style: &StrokeStyle,
        shader: Option<&Shader>,
        mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        PreparedPath::new(self, path, color, shader, mode)?
            .stroke(width, style)
            .map_err(Into::into)
    }

    fn set_clip_region(
        &mut self,
        path: &CanvasPath<'_>,
        rule: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        let path = web_path::prepare(path)?;
        let rule = winding(rule);
        self.context.clip_with_path_2d_and_winding(&path, rule);
        self.state.clips.push(Clip { path, rule });
        Ok(())
    }

    fn save(&mut self) -> Result<(), PdfCanvasError> {
        self.state.reserve_save()?;
        self.context.save();
        self.state.push_saved();
        Ok(())
    }

    fn restore(&mut self) -> Result<(), PdfCanvasError> {
        self.state.pop_saved()?;
        self.context.restore();
        Ok(())
    }

    fn draw_image_rect(
        &mut self,
        image: &Image,
        mode: Option<BlendMode>,
        rect: Rect,
        rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError> {
        ImageDraw::new(self, image, mode, rect, rotation)?
            .draw()
            .map_err(Into::into)
    }

    fn with_mask_layer<F>(&mut self, mask: &MaskLayer, paint: F) -> Result<(), PdfCanvasError>
    where
        F: FnOnce(&mut Self) -> Result<(), PdfCanvasError>,
    {
        if mask.is_passthrough() {
            return paint(self);
        }
        IsolatedMask::new(self, mask)?.paint(paint)
    }
}

/// A local browser save, separate from the PDF save/clip stack.
///
/// Declare the guard after paint resources: restoration releases Canvas's
/// references to their patterns before those backing surfaces are dropped.
#[must_use = "keep the guard alive until the drawing operation finishes"]
pub(crate) struct SavedContext<'a>(&'a CanvasRenderingContext2d);

impl<'a> SavedContext<'a> {
    pub(crate) fn new(context: &'a CanvasRenderingContext2d) -> Self {
        context.save();
        Self(context)
    }
}

impl Drop for SavedContext<'_> {
    fn drop(&mut self) {
        self.0.restore();
    }
}

/// Geometry and paint prepared for one particular target and device mapping.
///
/// Preparation may replay shaders, but does not change the destination's styles.
/// Consuming fill/stroke operations restore those styles before releasing paint.
struct PreparedPath<'a> {
    context: &'a CanvasRenderingContext2d,
    viewport: &'a CanvasViewport,
    geometry: Path2d,
    paint: PathPaint,
}

impl<'a> PreparedPath<'a> {
    fn new(
        backend: &'a WebCanvasBackend,
        path: &CanvasPath<'_>,
        color: Color,
        shader: Option<&Shader>,
        mode: Option<BlendMode>,
    ) -> WebResult<Self> {
        let geometry = web_path::prepare(path)?;
        let paint = PathPaint::new(backend, color, shader, mode)?;
        Ok(Self {
            context: &backend.context,
            viewport: &backend.viewport,
            geometry,
            paint,
        })
    }

    fn fill(self, rule: PathFillType) -> WebResult<()> {
        let _saved = SavedContext::new(self.context);
        self.paint.apply(self.context)?;
        self.context
            .fill_with_path_2d_and_winding(&self.geometry, winding(rule));
        Ok(())
    }

    fn stroke(self, width: f32, style: &StrokeStyle) -> WebResult<()> {
        let stroke = PreparedStroke::new(width, style, self.viewport)?;
        let _saved = SavedContext::new(self.context);
        self.paint.apply(self.context)?;
        stroke.apply(self.context)?;
        self.context.stroke_with_path(&self.geometry);
        Ok(())
    }
}

/// Paint owns both the source and its compositing operation.
struct PathPaint {
    source: PathSource,
    composite: &'static str,
}

/// A browser pattern must retain the raster from which it was created.
enum PathSource {
    Solid(String),
    Pattern {
        pattern: CanvasPattern,
        _raster: ShaderRaster,
        opacity: f64,
    },
}

impl PathPaint {
    fn new(
        backend: &WebCanvasBackend,
        color: Color,
        shader: Option<&Shader>,
        mode: Option<BlendMode>,
    ) -> WebResult<Self> {
        let source = match shader {
            Some(shader) => {
                let raster = ShaderRaster::new(backend, shader)?;
                let pattern =
                    raster.pattern(&backend.context, backend.viewport.device_to_backing())?;
                PathSource::Pattern {
                    pattern,
                    _raster: raster,
                    opacity: f64::from(color.a.clamp(0.0, 1.0)),
                }
            }
            None => PathSource::Solid(web_paint::color_css(color)?),
        };
        Ok(Self {
            source,
            composite: web_paint::blend(mode.as_ref()),
        })
    }

    fn apply(&self, context: &CanvasRenderingContext2d) -> WebResult<()> {
        context.set_global_composite_operation(self.composite)?;
        match &self.source {
            PathSource::Solid(css) => {
                context.set_global_alpha(1.0);
                context.set_fill_style_str(css);
                context.set_stroke_style_str(css);
            }
            PathSource::Pattern {
                pattern, opacity, ..
            } => {
                context.set_global_alpha(*opacity);
                context.set_fill_style_canvas_pattern(pattern);
                context.set_stroke_style_canvas_pattern(pattern);
            }
        }
        Ok(())
    }
}

/// A finite stroke width and dash data resolved for the target mapping.
/// The borrowed style retains cap/join metadata without cloning its dash vector.
struct PreparedStroke<'a> {
    width: f64,
    style: &'a StrokeStyle,
    dash: js_sys::Array,
    phase: f64,
}

impl<'a> PreparedStroke<'a> {
    fn new(width: f32, style: &'a StrokeStyle, viewport: &CanvasViewport) -> WebResult<Self> {
        let width = device_stroke_width(width, viewport.hairline_width())
            .ok_or(Error::InvalidInput("stroke width"))?;
        let (dash, phase) = match &style.dash_pattern {
            Some(pattern) => (
                pattern
                    .intervals
                    .iter()
                    .map(|&interval| wasm_bindgen::JsValue::from_f64(f64::from(interval)))
                    .collect(),
                f64::from(pattern.phase),
            ),
            None => (js_sys::Array::new(), 0.0),
        };
        Ok(Self {
            width: f64::from(width),
            style,
            dash,
            phase,
        })
    }

    fn apply(&self, context: &CanvasRenderingContext2d) -> WebResult<()> {
        context.set_line_width(self.width);
        context.set_line_cap(match self.style.line_cap {
            pdf_graphics::LineCap::Butt => "butt",
            pdf_graphics::LineCap::Round => "round",
            pdf_graphics::LineCap::Square => "square",
        });
        context.set_line_join(match self.style.line_join {
            pdf_graphics::LineJoin::Miter => "miter",
            pdf_graphics::LineJoin::Round => "round",
            pdf_graphics::LineJoin::Bevel => "bevel",
        });
        context.set_miter_limit(f64::from(self.style.miter_limit));
        context.set_line_dash_offset(self.phase);
        context.set_line_dash(&self.dash)?;
        Ok(())
    }
}

/// Maps a PDF fill rule to the browser winding rule.
pub(crate) fn winding(rule: PathFillType) -> CanvasWindingRule {
    match rule {
        PathFillType::Winding => CanvasWindingRule::Nonzero,
        PathFillType::EvenOdd => CanvasWindingRule::Evenodd,
    }
}
