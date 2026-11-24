//! PDF operator interpretation and painting orchestration.
use pdf_color_space::color_space::ColorSpace;
use std::{collections::HashMap, sync::Arc};

use crate::font_selection::FontSelection;
use crate::prepared_paint::{FillPaint, PreparedPaint, StrokePaint};
use crate::shading::shading_bbox_clip;
use crate::{
    CanvasPath,
    canvas_backend::{CanvasBackend, Shader},
    canvas_state::CanvasState,
    content_stream_render_state::ContentStreamRenderState,
    error::PdfCanvasError,
    recording_canvas::{RecordingCanvas, fits_offscreen_recording},
    stroke_style::StrokeStyle,
    text::TextGlyph,
    tiling_shader::{TilingShader, TilingShaderError, validate_geometry},
};
use pdf_content_stream::ContentStream;
use pdf_content_stream_operators::variants::PdfOperatorVariant;
use pdf_document::page::PdfPage;
use pdf_font::pdf_font_handle::PdfFontHandle;
use pdf_font::{FontError, FontFaceId, GlyphGeometry, GlyphId, PdfFontSpec, Standard14Font};
use pdf_graphics::{
    AlphaConstant, BlendMode, DashPattern, Image, PaintMode, PathFillType, color::Color,
    pdf_path::PdfPath, rect::Rect, size::Size, transform::Transform,
};
use pdf_optional_content::OptionalContentState;

use crate::image_cache::ImageCache;
use crate::optional_content_filter::OptionalContentFilter;
use pdf_resources::{
    form::FormXObject,
    image_xobject::ImageXObject,
    pattern::{PaintType, Pattern},
    resources::Resources,
};
use pdf_shading::{model::Shading, paint::build_shading_paint};
use pdf_text_engine::FontSystem;

/// How the active pattern paints a fill or stroke.
pub(crate) enum PatternPaint {
    /// No pattern applies; paint with the current color.
    Color,
    /// Paint through the pattern's shader.
    Shader(Shader),
    /// The pattern's cell or repeat step is empty, so painting leaves no marks.
    Nothing,
}

impl PatternPaint {
    /// Returns the shader to paint through, or `None` to paint the current color.
    pub(crate) fn into_shader(self) -> Option<Shader> {
        match self {
            Self::Shader(shader) => Some(shader),
            Self::Color | Self::Nothing => None,
        }
    }
}

/// Interprets PDF operators and prepares device-space painting for a borrowed backend.
pub struct PdfCanvas<'a, B: CanvasBackend> {
    /// The path being constructed; empty when no path is open.
    ///
    /// Its verb buffer is reused across paths that end without becoming a clip.
    pub(crate) current_path: PdfPath,
    /// Clip rule requested by `W`/`W*`, applied to the current path once the next
    /// path-painting operator ends it.
    ///
    /// The mark applies even when `W`/`W*` comes before the path is built. If that
    /// operator ends no path, the mark is dropped and the clipping path stays unchanged.
    pub(crate) pending_clip: Option<PathFillType>,
    /// Glyph outlines, in device space, accumulated by clip-mode text (modes 4–7) and
    /// applied as a clip when the text object ends (`ET`).
    ///
    /// Like the current path, this is text-object state rather than graphics state, so
    /// `q`/`Q` neither copy nor restore it.
    pub(crate) pending_text_clip: Option<PdfPath>,
    /// The drawing backend implementing `CanvasBackend` for rendering operations.
    pub(crate) canvas: &'a mut B,
    /// The PDF page associated with this canvas.
    pub(crate) page: &'a PdfPage,
    /// Shared text engine used to load and lay out font resources.
    pub(crate) font_system: Arc<FontSystem>,
    /// Loaded font handles keyed by the font they were selected as.
    font_cache: HashMap<FontKey, Arc<PdfFontHandle>>,
    /// Decoded image XObjects, shared with nested canvases and, through
    /// [`Self::with_image_cache`], with other renders.
    image_cache: Arc<ImageCache>,
    /// Scalable outlines reused by repeated glyphs during this page render.
    glyph_cache: HashMap<(FontFaceId, GlyphId), Arc<PdfPath>>,
    /// The last shading-pattern shader, reused while the same pattern fills at the same
    /// transform, such as across the glyphs of one text run.
    pattern_shading_cache: Option<(Arc<Pattern>, Transform, Shader)>,
    /// The last tiling-pattern shader, with the pattern, pattern transform and cell color it
    /// was recorded for, reused while fills or strokes keep all three.
    ///
    /// Holding the pattern keeps its address unique, so pointer equality identifies it.
    tiling_shader_cache: Option<(Arc<Pattern>, Transform, Option<Color>, TilingShader)>,
    /// The last dash pattern scaled into stroke space: the paint's pattern, the scale, and
    /// the scaled result, reused while strokes keep the same dash at the same scale.
    ///
    /// Holding the source pattern keeps its address unique, so pointer equality identifies it.
    scaled_dash_cache: Option<(Arc<DashPattern>, f32, Arc<DashPattern>)>,
    /// The current graphics state.
    pub(crate) state: CanvasState,
    /// Graphics states saved by `save`, innermost last.
    saved_states: Vec<CanvasState>,
    /// Number of saved states that a restore in the current content stream may not pop below.
    restore_floor: usize,
    /// Content streams being drawn, used to bound nesting and refuse cycles.
    content_stream_render_state: ContentStreamRenderState,
    /// Optional owned buffer for extracted text glyph positions.
    pub(crate) text_glyphs: Option<Vec<TextGlyph>>,
    /// Hides content whose optional content is off; `None` when nothing can be hidden.
    pub(crate) optional_content: Option<OptionalContentFilter<'a>>,
    /// Resources whose fonts `Tf` selects when the current resources lack the name.
    pub(crate) fallback_fonts: Option<Arc<Resources>>,
}

/// Identifies a loaded font in the canvas font cache.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum FontKey {
    /// A resource specification, by its address; the cached handle keeps it alive, so
    /// the address stays unique while the entry exists.
    Resource(*const PdfFontSpec),
    /// A Standard 14 font substituted for a name no resources define.
    Standard14(Standard14Font),
}

impl<'a, B: CanvasBackend> PdfCanvas<'a, B> {
    /// Creates a new `PdfCanvas` for rendering PDF graphics onto a backend surface.
    ///
    /// # Parameters
    ///
    /// - `backend`: The drawing backend implementing `CanvasBackend`.
    /// - `page`: The PDF page to render.
    /// - `bb`: Optional bounding box to override the page's media box.
    ///
    /// # Returns
    ///
    /// A new `PdfCanvas` instance or an error if the page dimensions are invalid.
    pub fn new(
        backend: &'a mut B,
        page: &'a PdfPage,
        bb: Option<&Rect>,
        font_system: Arc<FontSystem>,
    ) -> Result<Self, PdfCanvasError> {
        let viewport = page.viewport(bb, Size::new(backend.width(), backend.height()))?;
        Self::new_with_viewport(backend, page, &viewport, font_system)
    }

    /// Creates a canvas using the page mapping shared with layout and annotations.
    /// Returns an error if the backend has different logical device dimensions.
    pub fn new_with_viewport(
        backend: &'a mut B,
        page: &'a PdfPage,
        viewport: &crate::PageViewport,
        font_system: Arc<FontSystem>,
    ) -> Result<Self, PdfCanvasError> {
        if viewport.device_size() != Size::new(backend.width(), backend.height()) {
            return Err(crate::ViewportError::DeviceSizeMismatch.into());
        }
        let transform = *viewport.page_to_device();

        let state = CanvasState {
            transform,
            pattern_parent_transform: transform,
            ..Default::default()
        };

        Ok(Self {
            current_path: PdfPath::default(),
            pending_clip: None,
            pending_text_clip: None,
            canvas: backend,
            page,
            font_system,
            font_cache: HashMap::new(),
            image_cache: Arc::default(),
            glyph_cache: HashMap::new(),
            pattern_shading_cache: None,
            tiling_shader_cache: None,
            scaled_dash_cache: None,
            state,
            saved_states: Vec::new(),
            restore_floor: 0,
            content_stream_render_state: ContentStreamRenderState::default(),
            text_glyphs: None,
            optional_content: None,
            fallback_fonts: None,
        })
    }

    /// Enables collection of selectable glyph metadata during rendering.
    #[must_use]
    pub fn with_text_recording(mut self) -> Self {
        self.text_glyphs = Some(Vec::new());
        self
    }

    /// Draws image XObjects from `cache`, so renders sharing it decode each image once.
    ///
    /// Without a shared cache, each render decodes the images it draws.
    #[must_use]
    pub fn with_image_cache(mut self, cache: Arc<ImageCache>) -> Self {
        self.image_cache = cache;
        self
    }

    /// Hides content in marked-content sequences whose optional content is off in `state`.
    #[must_use]
    pub fn with_optional_content(mut self, state: &'a OptionalContentState) -> Self {
        self.optional_content = Some(OptionalContentFilter::new(state));
        self
    }

    /// Resolves a `Tf` font name the current resources lack in `resources`, as widget
    /// appearances resolve theirs in the interactive form's `/DR`.
    #[must_use]
    pub fn with_fallback_fonts(mut self, resources: Option<Arc<Resources>>) -> Self {
        self.fallback_fonts = resources;
        self
    }

    /// Returns whether an open marked-content sequence hides the content being drawn.
    pub(crate) fn content_hidden(&self) -> bool {
        self.optional_content
            .as_ref()
            .is_some_and(OptionalContentFilter::is_hidden)
    }

    /// Takes the glyph metadata collected by [`Self::with_text_recording`].
    ///
    /// Returns an empty vector when recording was not enabled.
    pub fn take_text_glyphs(&mut self) -> Vec<TextGlyph> {
        self.text_glyphs.take().unwrap_or_default()
    }

    /// Returns the backend's logical device area, `[0, width] x [0, height]`.
    pub(crate) fn device_rect(&self) -> Rect {
        Rect::new(self.canvas.width(), self.canvas.height())
    }

    /// Runs a form's content stream under `matrix`, clipped to the form's bounds and
    /// drawn with its resources.
    pub(crate) fn render_form(
        &mut self,
        form: &FormXObject,
        matrix: Option<Transform>,
    ) -> Result<(), PdfCanvasError> {
        self.render_content_stream(
            &form.content_stream,
            matrix,
            form.bbox.as_ref(),
            form.resources
                .as_ref()
                .map(|resources| resources.get())
                .transpose()?,
            None,
        )
    }

    /// Records a PDF content stream into an offscreen [`RecordingCanvas`].
    ///
    /// This helper is intended for rendering intermediate layers (e.g. pattern tiles or mask
    /// layers) into a temporary surface that will later be consumed by a backend client.
    ///
    /// **Coordinate system**
    ///
    /// Unlike [`PdfCanvas::new`], which flips the Y axis to map PDF user space (origin at
    /// bottom-left) into a typical device space, this method renders into a coordinate system
    /// whose origin is the **top-left** (device-style, like Skia).
    ///
    /// Concretely, the recording is the size of `bbox` mapped through `mat` and starts in
    /// the stream's content space mapped only by `mat`, without a Y-axis flip; clipping to
    /// `bbox` is applied by `render_content_stream` after `mat`.
    ///
    /// This matches the expectation that the consumer of the resulting `RecordingCanvas`
    /// (e.g. a shader/mask client in the backend) also uses a top-left-origin coordinate system.
    ///
    /// # Parameters
    ///
    /// - `content_stream`: The content stream containing the PDF operators to execute.
    /// - `mat`: Optional additional matrix (applied like a PDF `cm` / XObject `/Matrix`).
    /// - `bbox`: The content-space bounding box that clips the stream and, mapped through
    ///   `mat`, sizes the recording surface.
    /// - `resources`: Optional resource dictionary for resolving fonts, patterns, etc.
    /// - `filter`: Optional filter function to skip certain operations.
    /// - `cell_color`: Optional initial fill and stroke color, used by uncolored tiling
    ///   patterns whose cell is painted in the color selected with the pattern.
    ///
    /// # Errors
    ///
    /// Returns [`PdfCanvasError`] if rendering fails or the stream contains unsupported
    /// operations.
    pub(crate) fn record_content_stream(
        &mut self,
        content_stream: &ContentStream,
        mat: Option<Transform>,
        bbox: &Rect,
        resources: Option<Arc<Resources>>,
        filter: Option<&mut (dyn FnMut(&PdfOperatorVariant) -> bool + '_)>,
        cell_color: Option<Color>,
    ) -> Result<RecordingCanvas, PdfCanvasError> {
        let size = mat.map_or(*bbox, |mat| mat.map_rect(bbox)).size();
        let mut initial_state = CanvasState::default();
        if let Some(color) = cell_color {
            initial_state.paint.set_fill_color(color);
            initial_state.paint.set_stroke_color(color);
        }
        // Render the form's content stream into the mask canvas.
        self.record_nested(size, initial_state, None, |canvas| {
            canvas.render_content_stream(content_stream, mat, Some(bbox), resources, filter)
        })
    }

    /// Records `paint` on a nested canvas that starts from `initial_state`.
    ///
    /// The nested canvas shares this canvas's caches and active streams, so a cycle
    /// through a nested canvas is still detected. `text_glyphs` is the buffer it collects
    /// selectable text into, handed back to this canvas afterwards.
    pub(crate) fn record_nested(
        &mut self,
        size: Size<f32>,
        initial_state: CanvasState,
        text_glyphs: Option<Vec<TextGlyph>>,
        paint: impl FnOnce(&mut PdfCanvas<'_, RecordingCanvas>) -> Result<(), PdfCanvasError>,
    ) -> Result<RecordingCanvas, PdfCanvasError> {
        let mut recording_canvas = RecordingCanvas::new(size.width, size.height);
        let mut other = PdfCanvas::<RecordingCanvas> {
            current_path: PdfPath::default(),
            pending_clip: None,
            pending_text_clip: None,
            canvas: &mut recording_canvas,
            page: self.page,
            font_system: Arc::clone(&self.font_system),
            font_cache: std::mem::take(&mut self.font_cache),
            image_cache: Arc::clone(&self.image_cache),
            glyph_cache: std::mem::take(&mut self.glyph_cache),
            pattern_shading_cache: None,
            tiling_shader_cache: None,
            scaled_dash_cache: None,
            state: initial_state,
            saved_states: Vec::new(),
            restore_floor: 0,
            content_stream_render_state: std::mem::take(&mut self.content_stream_render_state),
            text_glyphs,
            optional_content: self
                .optional_content
                .as_ref()
                .map(OptionalContentFilter::restarted),
            fallback_fonts: self.fallback_fonts.clone(),
        };

        let result = paint(&mut other);
        self.font_cache = other.font_cache;
        self.glyph_cache = other.glyph_cache;
        self.content_stream_render_state = other.content_stream_render_state;
        self.text_glyphs = other.text_glyphs.or_else(|| self.text_glyphs.take());
        result.map(|()| recording_canvas)
    }

    /// Loads the font `name` selects for a content stream drawn with `resources`, as `Tf`
    /// does, with the nested resources of a Type 3 font.
    ///
    /// # Errors
    /// Propagates font loading failures.
    pub fn load_named_font(
        &mut self,
        name: &[u8],
        resources: Option<&Resources>,
    ) -> Result<(Arc<PdfFontHandle>, Option<Arc<Resources>>), PdfCanvasError> {
        match FontSelection::new(name, resources, self.fallback_fonts.as_deref()) {
            FontSelection::Resource { font, resources } => {
                Ok((self.load_pdf_font(&font)?, resources))
            }
            selection @ FontSelection::Standard14(font) => {
                let key = FontKey::Standard14(font);
                Ok((self.load_cached_font(key, || selection.spec())?, None))
            }
        }
    }

    /// Loads a PDF font once for each resource specification used by this canvas.
    pub(crate) fn load_pdf_font(
        &mut self,
        spec: &Arc<PdfFontSpec>,
    ) -> Result<Arc<PdfFontHandle>, PdfCanvasError> {
        let key = FontKey::Resource(Arc::as_ptr(spec));
        self.load_cached_font(key, || Arc::clone(spec))
    }

    /// The cached handle of `key`, loading the specification `spec` returns on a miss.
    fn load_cached_font(
        &mut self,
        key: FontKey,
        spec: impl FnOnce() -> Arc<PdfFontSpec>,
    ) -> Result<Arc<PdfFontHandle>, PdfCanvasError> {
        if let Some(handle) = self.font_cache.get(&key) {
            return Ok(Arc::clone(handle));
        }
        let handle = Arc::new(self.font_system.load_pdf_font(spec())?);
        self.font_cache.insert(key, Arc::clone(&handle));
        Ok(handle)
    }

    /// Decodes an image XObject unless the image cache holds its raster.
    pub(crate) fn decode_image_xobject(
        &self,
        image: &Arc<ImageXObject>,
    ) -> Result<Option<Arc<Image>>, PdfCanvasError> {
        self.image_cache.get_or_decode(image)
    }

    /// Returns a page-scoped cached scalable outline.
    pub(crate) fn glyph_outline(
        &mut self,
        face: &dyn pdf_font::FontFace,
        glyph: GlyphId,
    ) -> Result<Arc<PdfPath>, PdfCanvasError> {
        let key = (face.id(), glyph);
        if let Some(outline) = self.glyph_cache.get(&key) {
            return Ok(Arc::clone(outline));
        }
        let geometry = match face.glyph_geometry(glyph) {
            Ok(geometry) => geometry,
            Err(FontError::InvalidProgram { .. }) => None,
            Err(error) => return Err(error.into()),
        };
        let Some(GlyphGeometry::Outline(outline)) = geometry else {
            return Ok(Arc::new(PdfPath::default()));
        };
        let outline = Arc::new(outline);
        self.glyph_cache.insert(key, Arc::clone(&outline));
        Ok(outline)
    }

    /// Builds a shader from a parsed shading definition.
    ///
    /// # Parameters
    ///
    /// - `shading`: The shading pattern definition.
    /// - `matrix`: Optional transformation matrix for the shading.
    /// - `use_background`: Whether the shading's `/Background` fills uncovered areas,
    ///   which applies to shading patterns but not to the `sh` operator.
    ///
    /// # Returns
    ///
    /// An appropriate `Shader` if supported, or an error if not implemented.
    pub(crate) fn build_shading_shader(
        &mut self,
        shading: &Shading,
        transform: &Option<Transform>,
        use_background: bool,
    ) -> Result<Shader, PdfCanvasError> {
        build_shading_paint(shading, *transform, use_background)
            .map(Shader::Shading)
            .map_err(|error| PdfCanvasError::UnsupportedFeature(error.to_string()))
    }

    /// Returns the transform that maps `pattern` space into logical device space.
    ///
    /// Patterns are anchored to their parent stream's default user space,
    /// independently of later `cm` operators. Paths already use logical
    /// device coordinates, so shaders are prepared in the same coordinates.
    fn pattern_transform(state: &CanvasState, pattern: &Pattern) -> Transform {
        let transform = state.pattern_parent_transform;
        pattern
            .matrix()
            .map_or(transform, |matrix| transform.post_concatenated(matrix))
    }

    /// Returns the device-space clip of the active shading pattern's `/BBox`, if any.
    pub(crate) fn pattern_bbox_clip(
        &self,
        for_stroke: bool,
    ) -> Result<Option<CanvasPath<'static>>, PdfCanvasError> {
        let state = &self.state;
        let pattern = if for_stroke {
            &state.stroke_pattern
        } else {
            &state.fill_pattern
        };
        Ok(match pattern.as_deref() {
            Some(pattern @ Pattern::Shading { shading, .. }) => {
                shading_bbox_clip(shading, &Self::pattern_transform(state, pattern))?
            }
            _ => None,
        })
    }

    /// Computes how the active pattern paints a fill or stroke.
    ///
    /// # Returns
    ///
    /// The [`PatternPaint`] to use, or an error if pattern rendering fails.
    pub(crate) fn compute_shader(
        &mut self,
        for_stroke: bool,
    ) -> Result<PatternPaint, PdfCanvasError> {
        let state = &self.state;
        let pattern = if for_stroke {
            state.stroke_pattern.clone()
        } else {
            state.fill_pattern.clone()
        };

        let Some(pattern) = pattern else {
            return Ok(PatternPaint::Color);
        };
        let transform = Self::pattern_transform(state, &pattern);

        match pattern.as_ref() {
            Pattern::Shading { shading, .. } => {
                if let Some((cached, cached_transform, shader)) = &self.pattern_shading_cache
                    && Arc::ptr_eq(cached, &pattern)
                    && *cached_transform == transform
                {
                    return Ok(PatternPaint::Shader(shader.clone()));
                }
                let shader = self.build_shading_shader(shading, &Some(transform), true)?;
                self.pattern_shading_cache =
                    Some((Arc::clone(&pattern), transform, shader.clone()));
                Ok(PatternPaint::Shader(shader))
            }
            Pattern::Tiling {
                bbox,
                resources,
                content_stream,
                x_step,
                y_step,
                paint_type,
                ..
            } => {
                let bbox = *bbox;
                let step = [*x_step, *y_step];
                // An empty cell or a zero repeat step tiles nothing.
                if validate_geometry(&bbox, step).is_err() {
                    return Ok(PatternPaint::Nothing);
                }
                if !fits_offscreen_recording(&bbox) {
                    return Ok(PatternPaint::Color);
                }

                // The cell of an uncolored pattern is painted in the color selected
                // together with the pattern.
                let paint = &self.state.paint;
                let cell_color = match paint_type {
                    PaintType::Colored => None,
                    PaintType::Uncolored if for_stroke => Some(paint.stroke_color),
                    PaintType::Uncolored => Some(paint.fill_color),
                };

                if let Some((cached, cached_transform, cached_color, shader)) =
                    &self.tiling_shader_cache
                    && Arc::ptr_eq(cached, &pattern)
                    && *cached_transform == transform
                    && *cached_color == cell_color
                {
                    return Ok(PatternPaint::Shader(Shader::TilingPatternImage(
                        shader.clone(),
                    )));
                }
                let shader = self.record_tiling_shader(
                    content_stream,
                    resources.get()?,
                    bbox,
                    step,
                    transform,
                    cell_color,
                )?;
                self.tiling_shader_cache =
                    Some((Arc::clone(&pattern), transform, cell_color, shader.clone()));
                Ok(PatternPaint::Shader(Shader::TilingPatternImage(shader)))
            }
        }
    }

    /// Records a tiling pattern's cell and prepares the shader that repeats it.
    ///
    /// A `cell_color` marks an uncolored pattern: the cell is painted in that color, and
    /// the cell's own color operators are skipped.
    ///
    /// # Errors
    /// Returns an error if recording the cell fails or the tiling geometry is invalid.
    fn record_tiling_shader(
        &mut self,
        content_stream: &ContentStream,
        resources: Arc<Resources>,
        bbox: Rect,
        step: [f32; 2],
        transform: Transform,
        cell_color: Option<Color>,
    ) -> Result<TilingShader, PdfCanvasError> {
        let mut uncolored_filter = PdfOperatorVariant::sets_color;
        let filter: Option<&mut dyn FnMut(&PdfOperatorVariant) -> bool> = match cell_color {
            Some(_) => Some(&mut uncolored_filter),
            None => None,
        };
        let recording_canvas = self.record_content_stream(
            content_stream,
            None,
            &bbox,
            Some(resources),
            filter,
            cell_color,
        )?;

        Ok(
            match TilingShader::new(Arc::new(recording_canvas), transform, bbox, step) {
                // A cell too costly to replay, such as one built from deeply
                // nested patterns, paints nothing instead of failing the page.
                Err(TilingShaderError::ReplayLimitExceeded) => TilingShader::new(
                    Arc::new(RecordingCanvas::new(bbox.width(), bbox.height())),
                    transform,
                    bbox,
                    step,
                )?,
                shader => shader?,
            },
        )
    }

    /// Draws a path using the specified paint mode and fill type, applying any active shader or pattern.
    ///
    /// # Parameters
    ///
    /// - `path`: The path to draw.
    /// - `mode`: The paint mode (fill, stroke, or fill and stroke).
    /// - `fill_type`: The fill rule to use.
    ///
    /// # Errors
    ///
    /// Returns an error if the paint mode is not implemented or if pattern computation fails.
    pub(crate) fn draw_path(
        &mut self,
        path: &CanvasPath<'_>,
        mode: PaintMode,
        fill_type: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        let paint = self.prepare_paint(mode)?;
        self.paint_prepared(path, fill_type, &paint)
    }

    /// Works out the fill and stroke `mode` applies from the current graphics state.
    ///
    /// # Errors
    /// Returns an error if pattern computation fails or the dash pattern is invalid.
    pub(crate) fn prepare_paint(
        &mut self,
        mode: PaintMode,
    ) -> Result<PreparedPaint, PdfCanvasError> {
        let fill = match mode {
            PaintMode::Fill | PaintMode::FillAndStroke => self.prepare_fill()?,
            PaintMode::Stroke => None,
        };
        let stroke = match mode {
            PaintMode::Stroke | PaintMode::FillAndStroke => self.prepare_stroke()?,
            PaintMode::Fill => None,
        };
        Ok(PreparedPaint {
            fill,
            stroke,
            blend_mode: self.state.paint.blend_mode,
        })
    }

    /// Works out the current fill, or `None` when the fill pattern leaves no marks.
    fn prepare_fill(&mut self) -> Result<Option<FillPaint>, PdfCanvasError> {
        let shader = match self.compute_shader(false)? {
            PatternPaint::Nothing => return Ok(None),
            pattern_paint => pattern_paint.into_shader(),
        };
        let paint = &self.state.paint;
        Ok(Some(FillPaint {
            color: paint.fill_color,
            shader,
            clip: self.pattern_bbox_clip(false)?,
        }))
    }

    /// Works out the current stroke, or `None` when the stroke pattern leaves no marks.
    fn prepare_stroke(&mut self) -> Result<Option<StrokePaint>, PdfCanvasError> {
        let shader = match self.compute_shader(true)? {
            PatternPaint::Nothing => return Ok(None),
            pattern_paint => pattern_paint.into_shader(),
        };
        let scale = self.state.transform.area_scale();
        let dash_pattern = self.scaled_dash_pattern(scale)?;
        let state = &self.state;
        Ok(Some(StrokePaint {
            color: state.paint.stroke_color,
            line_width: state.paint.line_width * scale,
            style: StrokeStyle::new(&state.paint, &state.transform, dash_pattern),
            shader,
            clip: self.pattern_bbox_clip(true)?,
        }))
    }

    /// Paints `path` with `paint` through the current soft mask.
    ///
    /// # Errors
    /// Propagates backend failures.
    pub(crate) fn paint_prepared(
        &mut self,
        path: &CanvasPath<'_>,
        fill_type: PathFillType,
        paint: &PreparedPaint,
    ) -> Result<(), PdfCanvasError> {
        self.with_soft_mask(|backend| paint.paint(backend, path, fill_type))
    }

    /// Returns the current dash pattern scaled by `scale` into stroke space, or `None` for
    /// a solid stroke.
    ///
    /// The last result is reused while the paint keeps the same pattern at the same scale.
    ///
    /// # Errors
    /// Returns an error if the scaled intervals are invalid.
    fn scaled_dash_pattern(
        &mut self,
        scale: f32,
    ) -> Result<Option<Arc<DashPattern>>, PdfCanvasError> {
        let Some(source) = &self.state.paint.dash_pattern else {
            return Ok(None);
        };
        if let Some((cached_source, cached_scale, scaled)) = &self.scaled_dash_cache
            && Arc::ptr_eq(cached_source, source)
            && cached_scale.to_bits() == scale.to_bits()
        {
            return Ok(Some(Arc::clone(scaled)));
        }
        let scaled = Arc::new(source.scaled(scale)?);
        self.scaled_dash_cache = Some((Arc::clone(source), scale, Arc::clone(&scaled)));
        Ok(Some(scaled))
    }

    /// Borrows the backend for one painting operation with the currently selected mask.
    ///
    /// Callers prepare PDF state before this boundary so the callback needs only the backend.
    pub(crate) fn with_soft_mask(
        &mut self,
        paint: impl FnOnce(&mut B) -> Result<(), PdfCanvasError>,
    ) -> Result<(), PdfCanvasError> {
        // Hidden optional content still updates graphics state, but paints nothing.
        if self.content_hidden() {
            return Ok(());
        }
        match &self.state.soft_mask {
            Some(mask) => self.canvas.with_mask_layer(mask, paint),
            None => paint(self.canvas),
        }
    }

    /// Sets the alpha value for subsequent non-stroking paint operations.
    ///
    /// This updates only the fill alpha component in the current graphics state.
    pub fn set_non_stroking_alpha(&mut self, alpha: AlphaConstant) -> Result<(), PdfCanvasError> {
        self.state.paint.fill_color.a = alpha.value();
        Ok(())
    }

    /// Sets the alpha value for subsequent stroking paint operations.
    ///
    /// This updates only the stroke alpha component in the current graphics state.
    pub fn set_stroking_alpha(&mut self, alpha: AlphaConstant) -> Result<(), PdfCanvasError> {
        self.state.paint.stroke_color.a = alpha.value();
        Ok(())
    }

    /// Sets the blend mode for subsequent paint operations.
    pub fn set_blend_mode(&mut self, blend_mode: Option<BlendMode>) -> Result<(), PdfCanvasError> {
        self.state.paint.blend_mode = blend_mode;
        Ok(())
    }

    /// Ends the current path: paints it (if `paint` is given), then intersects the clip with
    /// it when `W`/`W*` marked it, and clears the path.
    ///
    /// The marked clip takes effect only after painting, so the painting itself is not
    /// restricted by it.
    ///
    /// # Parameters
    ///
    /// - `paint`: The paint mode and fill rule, or `None` to end the path without painting.
    ///
    /// # Errors
    ///
    /// Returns an error if drawing or clipping fails.
    pub(crate) fn end_path(
        &mut self,
        paint: Option<(PaintMode, PathFillType)>,
    ) -> Result<(), PdfCanvasError> {
        let pending_clip = self.pending_clip.take();
        if self.current_path.verbs.is_empty() {
            return Ok(());
        }
        let mut path = std::mem::take(&mut self.current_path);
        // Hidden optional content paints nothing, so skip preparing its paint.
        if let Some((mode, fill_type)) = paint
            && !self.content_hidden()
        {
            let device_path = CanvasPath::transformed(&path, self.state.transform)?;
            self.draw_path(&device_path, mode, fill_type)?;
        }
        match pending_clip {
            Some(clip_rule) => self.set_clip_path(path, clip_rule),
            None => {
                path.clear();
                self.current_path = path;
                Ok(())
            }
        }
    }

    /// Sets the clipping path for subsequent drawing operations.
    ///
    /// # Parameters
    ///
    /// - `path`: The path to use as the new clipping region.
    /// - `mode`: The fill rule for the clipping path.
    ///
    /// # Errors
    ///
    /// Returns an error if the graphics state is invalid.
    pub(crate) fn set_clip_path(
        &mut self,
        path: PdfPath,
        mode: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        let path = CanvasPath::shared(Arc::new(path), self.state.transform)?;
        self.canvas.set_clip_region(&path, mode)?;
        self.state.clip_path = Some(path);
        Ok(())
    }

    /// Appends a glyph outline, mapped into device space, to the pending text clip accumulator.
    ///
    /// Call this for every glyph rendered with a clip mode (4–7). The accumulated path
    /// is applied as a clip region when the text object ends via `end_text_object`.
    pub(crate) fn add_to_text_clip(&mut self, path: &CanvasPath<'_>) -> Result<(), PdfCanvasError> {
        let clip = self.pending_text_clip.get_or_insert_with(PdfPath::default);
        path.extend_into(clip)?;
        Ok(())
    }

    /// Sets the current fill pattern by name from the page resources.
    ///
    /// # Parameters
    ///
    /// - `pattern_name`: The name of the pattern to activate.
    ///
    /// # Errors
    ///
    /// Returns an error if the pattern is not found in the resources.
    pub(crate) fn set_fill_pattern(&mut self, pattern_name: &[u8]) -> Result<(), PdfCanvasError> {
        let Some(pattern) = self
            .state
            .resources
            .as_ref()
            .and_then(|r| r.pattern(pattern_name))
        else {
            return Err(PdfCanvasError::PatternNotFound(
                String::from_utf8_lossy(pattern_name).into_owned(),
            ));
        };
        self.state.fill_pattern = Some(pattern);
        Ok(())
    }

    /// Sets the current stroke pattern by name from the page resources.
    ///
    /// # Parameters
    ///
    /// - `pattern_name`: The name of the pattern to activate.
    ///
    /// # Errors
    ///
    /// Returns an error if the pattern is not found in the resources.
    pub(crate) fn set_stroke_pattern(&mut self, pattern_name: &[u8]) -> Result<(), PdfCanvasError> {
        let Some(pattern) = self
            .state
            .resources
            .as_ref()
            .and_then(|r| r.pattern(pattern_name))
        else {
            return Err(PdfCanvasError::PatternNotFound(
                String::from_utf8_lossy(pattern_name).into_owned(),
            ));
        };
        self.state.stroke_pattern = Some(pattern);
        Ok(())
    }

    /// Renders a sequence of PDF content stream operations onto the canvas.
    ///
    /// # Parameters
    ///
    /// - `content_stream`: The content stream containing the PDF operators to execute.
    /// - `mat`: Optional transformation matrix to apply.
    /// - `bbox`: Optional bounding box to clip the rendering.
    /// - `resources`: Optional resource dictionary to use for rendering.
    /// - `filter`: Optional filter function to skip certain operations.
    ///
    /// # Errors
    ///
    /// Returns an error if any operation fails or if the graphics state is invalid.
    pub fn render_content_stream(
        &mut self,
        content_stream: &ContentStream,
        mat: Option<Transform>,
        bbox: Option<&Rect>,
        resources: Option<Arc<Resources>>,
        filter: Option<&mut (dyn FnMut(&PdfOperatorVariant) -> bool + '_)>,
    ) -> Result<(), PdfCanvasError> {
        let Some(invocation) = self.content_stream_render_state.enter(content_stream.id) else {
            // A refused stream is a successful no-op so a cycle or excessive
            // nesting cannot abort the rest of the page.
            return Ok(());
        };

        let result =
            self.render_admitted_content_stream(content_stream, mat, bbox, resources, filter);

        // Release admission state even when setup or an operator returned an
        // error, allowing later streams to render normally on this canvas.
        self.content_stream_render_state.exit(invocation);

        result
    }

    /// Renders an admitted content stream and balances graphics state on exit.
    ///
    /// Restores in the stream cannot pop states saved by its caller, and states the
    /// stream saved without restoring are restored when it ends.
    fn render_admitted_content_stream(
        &mut self,
        content_stream: &ContentStream,
        mat: Option<Transform>,
        bbox: Option<&Rect>,
        resources: Option<Arc<Resources>>,
        mut filter: Option<&mut (dyn FnMut(&PdfOperatorVariant) -> bool + '_)>,
    ) -> Result<(), PdfCanvasError> {
        let depth = self.saved_states.len();
        let marked_content_depth = self
            .optional_content
            .as_ref()
            .map(OptionalContentFilter::depth);
        self.save()?;
        let outer_floor = std::mem::replace(&mut self.restore_floor, self.saved_states.len());
        let outer_text_clip = self.pending_text_clip.take();

        let result = (|| {
            if let Some(mat) = mat {
                // Concatenate the provided `XObject` matrix with the current CTM.
                // PDF spec: invoking a form XObject with its /Matrix entry performs a
                // concatenation like the 'cm' operator does. The operation is:
                //   CTM' = CTM * FormMatrix
                self.state.transform.post_concat(&mat);
            }

            let state = &mut self.state;
            state.pattern_parent_transform = state.transform;

            if let Some(bbox) = bbox {
                // Set up a clipping path based on the bounding box.
                self.set_clip_path(PdfPath::from(bbox), PathFillType::EvenOdd)?;
            }

            if let Some(resources) = resources {
                self.state.resources = Some(resources);
            }

            for op in &content_stream.operators {
                if filter.as_mut().is_some_and(|filter| filter(op)) {
                    continue;
                }
                self.apply_operator(op)?;
            }

            Ok(())
        })();
        // A Type 3 glyph procedure runs inside the caller's text object; its own text
        // objects must neither clear nor apply the caller's pending text clip.
        self.pending_text_clip = outer_text_clip;

        self.restore_floor = outer_floor;
        // Marked-content sequences cannot span streams, so close any left open.
        if let (Some(filter), Some(depth)) = (&mut self.optional_content, marked_content_depth) {
            filter.close_to(depth);
        }
        let restore_result = self.restore_to(depth);
        result.and(restore_result)
    }

    /// Restores saved graphics states until `depth` states remain saved.
    ///
    /// Stops at the first failed restore, which leaves the stack unchanged.
    fn restore_to(&mut self, depth: usize) -> Result<(), PdfCanvasError> {
        while self.saved_states.len() > depth.max(self.restore_floor) {
            self.restore()?;
        }
        Ok(())
    }

    /// Saves the entire current graphics state onto a stack.
    ///
    /// This includes the current transformation matrix, colors, line styles, and clipping path.
    /// A corresponding call to `restore` is required to pop the state from the stack.
    pub(crate) fn save(&mut self) -> Result<(), PdfCanvasError> {
        self.canvas.save()?;
        self.saved_states.push(self.state.clone());
        Ok(())
    }

    /// Restores the most recently saved graphics state from the stack.
    ///
    /// If the restored state included a clipping path, the clipping path is reset on the backend.
    pub(crate) fn restore(&mut self) -> Result<(), PdfCanvasError> {
        // Ignore unmatched restores: they may not pop the initial state, which has
        // no backend `save()`, nor states saved by the caller of the current
        // content stream. This keeps the canvas and backend stacks in sync.
        if self.saved_states.len() <= self.restore_floor {
            return Ok(());
        }

        // At this point the top state was saved within the current stream,
        // so popping is safe and has a matching backend `save()`.
        self.canvas.restore()?;
        if let Some(state) = self.saved_states.pop() {
            self.state = state;
        }
        Ok(())
    }

    /// Sets the current color space for stroking or filling operations.
    ///
    /// # Parameters
    ///
    /// - `name`: The name of the color space to set.  c
    /// - `is_stroking`: If `true`, sets the stroking color space; otherwise, sets the filling color space.
    pub(crate) fn set_color_space(
        &mut self,
        name: &[u8],
        is_stroking: bool,
    ) -> Result<(), PdfCanvasError> {
        let state = &mut self.state;
        if is_stroking {
            state.stroke_pattern = None;
        } else {
            state.fill_pattern = None;
        }

        // The names /DeviceGray, /DeviceRGB, /DeviceCMYK, and /Pattern are reserved
        // keywords that always identify their corresponding colour spaces directly.
        // Per PDF spec §8.6.8 each also sets the current colour to its initial value.
        let initial_color = match name {
            b"DeviceGray" => Some((ColorSpace::DeviceGray, Color::from_gray(0.0))),
            b"DeviceRGB" => Some((ColorSpace::DeviceRGB, Color::from_rgb(0.0, 0.0, 0.0))),
            b"DeviceCMYK" => Some((ColorSpace::DeviceCMYK, Color::from_cmyk(0.0, 0.0, 0.0, 1.0))),
            _ => None,
        };
        if let Some((space, color)) = initial_color {
            state.select_device_color_space(is_stroking, space);
            if is_stroking {
                state.paint.set_stroke_color(color);
            } else {
                state.paint.set_fill_color(color);
            }
            return Ok(());
        }
        if name == b"Pattern" {
            // Pattern: no initial colour is defined by the spec.
            let cs = Arc::new(ColorSpace::Pattern(None));
            if is_stroking {
                state.stroke_color_space = cs;
            } else {
                state.fill_color_space = cs;
            }
            return Ok(());
        }

        let Some(cs) = state
            .resources
            .as_ref()
            .and_then(|res| res.color_space(name))
        else {
            return Err(PdfCanvasError::ColorSpaceNotFound(
                String::from_utf8_lossy(name).into_owned(),
            ));
        };

        if is_stroking {
            state.stroke_color_space = cs;
        } else {
            state.fill_color_space = cs;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use pdf_document::page::PdfPage;
    use pdf_text_engine::bundled_font_system;

    use super::PdfCanvas;
    use crate::recording_canvas::RecordingCanvas;

    /// Creates a minimal page fixture for graphics-state tests.
    fn page() -> PdfPage {
        PdfPage {
            contents: None,
            annotations: None,
            media_box: None,
            crop_box: None,
            resources: None,
            rotation: None,
            annotation_id_high_watermark: 0,
        }
    }

    #[test]
    /// Verifies that set dash pattern updates current state.
    fn set_dash_pattern_updates_current_state() {
        let page = page();
        let mut backend = RecordingCanvas::new(100.0, 100.0);
        let mut canvas = PdfCanvas::new(&mut backend, &page, None, bundled_font_system())
            .expect("canvas should build");

        canvas
            .set_dash_pattern(&[4.0, 2.0], 1.0)
            .expect("dash pattern should be valid");

        let dash_pattern = canvas
            .state
            .paint
            .dash_pattern
            .as_ref()
            .expect("dash pattern should be stored");
        assert_eq!(dash_pattern.intervals, vec![4.0, 2.0]);
        assert_eq!(dash_pattern.phase, 1.0);
    }

    #[test]
    /// Verifies that empty dash pattern clears current state.
    fn empty_dash_pattern_clears_current_state() {
        let page = page();
        let mut backend = RecordingCanvas::new(100.0, 100.0);
        let mut canvas = PdfCanvas::new(&mut backend, &page, None, bundled_font_system())
            .expect("canvas should build");

        canvas
            .set_dash_pattern(&[4.0, 2.0], 1.0)
            .expect("dash pattern should be valid");
        canvas
            .set_dash_pattern(&[], 3.0)
            .expect("empty dash pattern should be valid");

        assert!(canvas.state.paint.dash_pattern.is_none());
    }

    #[test]
    /// Verifies that invalid dash pattern does not mutate current state.
    fn invalid_dash_pattern_does_not_mutate_current_state() {
        let page = page();
        let mut backend = RecordingCanvas::new(100.0, 100.0);
        let mut canvas = PdfCanvas::new(&mut backend, &page, None, bundled_font_system())
            .expect("canvas should build");

        canvas
            .set_dash_pattern(&[4.0, 2.0], 1.0)
            .expect("dash pattern should be valid");
        let result = canvas.set_dash_pattern(&[0.0, 0.0], 0.0);

        assert!(result.is_err());
        let dash_pattern = canvas
            .state
            .paint
            .dash_pattern
            .as_ref()
            .expect("previous dash pattern should remain");
        assert_eq!(dash_pattern.intervals, vec![4.0, 2.0]);
        assert_eq!(dash_pattern.phase, 1.0);
    }

    #[test]
    /// Verifies that a `W` issued before the path is built clips to the path ended by `n`.
    fn clip_before_path_construction_clips_to_ended_path() {
        use pdf_content_stream_operators::variants::PdfOperatorVariant;

        let page = page();
        let mut backend = RecordingCanvas::new(100.0, 100.0);
        let mut canvas = PdfCanvas::new(&mut backend, &page, None, bundled_font_system())
            .expect("canvas should build");

        let operators = [
            PdfOperatorVariant::ClipNonZero,
            PdfOperatorVariant::MoveTo { x: 40.0, y: 20.0 },
            PdfOperatorVariant::LineTo { x: 160.0, y: 20.0 },
            PdfOperatorVariant::LineTo { x: 160.0, y: 80.0 },
            PdfOperatorVariant::LineTo { x: 40.0, y: 80.0 },
            PdfOperatorVariant::ClosePath,
            PdfOperatorVariant::EndPath,
        ];
        for operator in &operators {
            canvas
                .apply_operator(operator)
                .expect("operator should apply");
        }

        assert!(canvas.state.clip_path.is_some());
    }

    #[test]
    /// Verifies that an inline image too short for its dimensions paints nothing.
    fn truncated_inline_image_paints_nothing() {
        use pdf_object_reader::{
            Dictionary, EmptySource, ObjectReader, StreamObject, object_variant::ObjectVariant,
            pdf_string::PdfString, string_kind::StringKind,
        };
        let dictionary = Dictionary::from_entries([
            (b"Width".as_slice(), ObjectVariant::Integer(60)),
            (b"Height".as_slice(), ObjectVariant::Integer(1)),
            (b"BitsPerComponent".as_slice(), ObjectVariant::Integer(8)),
            (
                b"ColorSpace".as_slice(),
                PdfString::from(b"DeviceRGB", StringKind::Name),
            ),
        ]);
        let stream = StreamObject::new(0, 0, dictionary, vec![0; 155]);
        let image: pdf_image::PdfImage = ObjectReader::new(EmptySource)
            .read(&ObjectVariant::Stream(stream))
            .expect("image metadata should read");
        let page = page();
        let mut backend = RecordingCanvas::new(100.0, 100.0);
        let mut canvas = PdfCanvas::new(&mut backend, &page, None, bundled_font_system())
            .expect("canvas should build");

        canvas
            .paint_inline_image(&pdf_image::InlineImage::Ready(image))
            .expect("truncated inline image should be skipped");
        drop(canvas);
        assert_eq!(backend.replay_cost(), 0);
    }

    #[test]
    /// Verifies that a tiling pattern with an empty cell and zero steps paints nothing.
    fn degenerate_tiling_pattern_paints_nothing() {
        use pdf_content_stream_operators::variants::PdfOperatorVariant;
        use pdf_object_reader::{
            Dictionary, EmptySource, ObjectReader, StreamObject, object_variant::ObjectVariant,
        };
        use pdf_resources::pattern::Pattern;

        let dictionary = Dictionary::from_entries([
            (b"PatternType".as_slice(), ObjectVariant::Integer(1)),
            (b"PaintType".as_slice(), ObjectVariant::Integer(2)),
            (b"TilingType".as_slice(), ObjectVariant::Integer(3)),
            (
                b"BBox".as_slice(),
                ObjectVariant::Array(vec![ObjectVariant::Integer(0); 4].into()),
            ),
            (b"XStep".as_slice(), ObjectVariant::Integer(0)),
            (b"YStep".as_slice(), ObjectVariant::Integer(0)),
        ]);
        let stream = StreamObject::new(0, 0, dictionary, b"q Q".to_vec());
        let pattern: Pattern = ObjectReader::new(EmptySource)
            .read(&ObjectVariant::Stream(stream))
            .expect("pattern should read");
        let page = page();
        let mut backend = RecordingCanvas::new(100.0, 100.0);
        let mut canvas = PdfCanvas::new(&mut backend, &page, None, bundled_font_system())
            .expect("canvas should build");
        canvas.state.fill_pattern = Some(std::sync::Arc::new(pattern));

        let operators = [
            PdfOperatorVariant::Rectangle {
                x: 10.0,
                y: 10.0,
                width: 50.0,
                height: 50.0,
            },
            PdfOperatorVariant::FillPathNonZero,
        ];
        for operator in &operators {
            canvas
                .apply_operator(operator)
                .expect("operator should apply");
        }
        drop(canvas);
        assert_eq!(backend.replay_cost(), 0);
    }
}
