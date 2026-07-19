use std::collections::HashSet;
use std::sync::Arc;

use crate::{
    canvas_backend::{CanvasBackend, Shader},
    canvas_state::CanvasState,
    error::PdfCanvasError,
    pdf_path_pen::PdfPathPen,
    recording_canvas::RecordingCanvas,
    stroke_style::StrokeStyle,
    text::{TextGlyph, TextSink, glyph_bounds, glyph_text},
    text_state::TextState,
};
use pdf_content_stream::ContentStream;
use pdf_content_stream_operators::pdf_operator_backend::PathConstructionOps;
use pdf_content_stream_operators::variants::PdfOperatorVariant;
use pdf_document::page::PdfPage;
use pdf_graphics::{
    BlendMode, MaskMode, PaintMode, PathFillType, TextRenderingMode, color::Color,
    pdf_path::PathVerb, pdf_path::PdfPath, rect::Rect, transform::Transform,
};
use pdf_resources::{
    pattern::{PaintType, Pattern},
    resources::Resources,
    shading::Shading,
};
use pdf_shading::paint::build_shading_paint;
use skrifa::{
    OutlineGlyph,
    outline::DrawSettings,
    prelude::{LocationRef, Size},
};

pub struct PdfCanvas<'a, B: CanvasBackend> {
    /// The current path being constructed or drawn, if any.
    pub(crate) current_path: Option<PdfPath>,
    /// The drawing backend implementing `CanvasBackend` for rendering operations.
    pub(crate) canvas: &'a mut B,
    /// An optional mask surface for advanced compositing or clipping.
    pub(crate) mask: Option<(Arc<RecordingCanvas>, MaskMode, Transform)>,
    /// The PDF page associated with this canvas.
    pub(crate) page: &'a PdfPage,
    /// The stack of graphics states, supporting save/restore semantics.
    pub(crate) canvas_stack: Vec<CanvasState<'a>>,
    /// Content-stream IDs currently being rendered on this canvas stack.
    pub(crate) active_content_stream_ids: HashSet<usize>,
    /// Optional sink for extracted text glyph positions.
    pub(crate) text_sink: Option<&'a mut dyn TextSink>,
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
    ) -> Result<Self, PdfCanvasError> {
        let media_box = &page.media_box;

        let (pdf_media_width, pdf_media_height) = if let Some(bb) = bb {
            (bb.width(), bb.height())
        } else if let Some(mb) = media_box.as_ref() {
            (mb.width(), mb.height())
        } else {
            (0.0, 0.0)
        };

        let backend_canvas_width = backend.width();
        let backend_canvas_height = backend.height();

        // Calculate scale factors.
        let scale_x = if pdf_media_width != 0.0 {
            backend_canvas_width / pdf_media_width
        } else {
            1.0
        };

        let scale_y = if pdf_media_height != 0.0 {
            backend_canvas_height / pdf_media_height
        } else {
            1.0
        };

        // Directly construct the userspace transformation matrix.
        // This matrix performs the following operations on PDF coordinates (px, py):
        // 1. Scales them: (px * scale_x, py * scale_y)
        // 2. Flips the Y-axis and translates it: Y_canvas = backend_canvas_height - (py * scale_y)
        // Resulting canvas coordinates: (px * scale_x, backend_canvas_height - py * scale_y)
        let transform = Transform::from_row(
            scale_x,               // sx: Scale X
            0.0,                   // ky: Skew Y (none)
            0.0,                   // kx: Skew X (none)
            -scale_y,              // sy: Scale Y and reflect (Y points down)
            0.0,                   // tx: Translate X (none)
            backend_canvas_height, // ty: Translate Y to move origin to top-left after reflection
        );

        let canvas_stack = vec![CanvasState {
            transform,
            text_state: TextState::default(),
            ..Default::default()
        }];

        Ok(Self {
            current_path: None,
            canvas: backend,
            mask: None,
            page,
            canvas_stack,
            active_content_stream_ids: HashSet::new(),
            text_sink: None,
        })
    }

    /// Creates a new `PdfCanvas` and emits rendered text spans into `text_sink`.
    pub fn new_with_text_sink(
        backend: &'a mut B,
        page: &'a PdfPage,
        bb: Option<&Rect>,
        text_sink: &'a mut dyn TextSink,
    ) -> Result<Self, PdfCanvasError> {
        let mut canvas = Self::new(backend, page, bb)?;
        canvas.text_sink = Some(text_sink);
        Ok(canvas)
    }

    /// Returns whether a form or pattern bbox is safe to materialize as an offscreen recording.
    ///
    /// PDFs in the wild sometimes contain malformed `/BBox` values for Form XObjects or tiling
    /// patterns, including inverted coordinates, zero-sized boxes, non-finite values, or sentinel
    /// coordinates near `±32768` that would expand into enormous temporary surfaces. This guard
    /// keeps those cases from turning into backend allocation failures by requiring a finite,
    /// positive bbox whose dimensions and total area stay within conservative offscreen limits.
    pub(crate) fn can_record_offscreen_bbox(bbox: &Rect) -> bool {
        const MAX_OFFSCREEN_RECORDING_DIMENSION: f32 = 8_192.0;
        const MAX_OFFSCREEN_RECORDING_AREA: f32 =
            MAX_OFFSCREEN_RECORDING_DIMENSION * MAX_OFFSCREEN_RECORDING_DIMENSION;

        let width = bbox.width();
        let height = bbox.height();

        width.is_finite()
            && height.is_finite()
            && width > 0.0
            && height > 0.0
            && width <= MAX_OFFSCREEN_RECORDING_DIMENSION
            && height <= MAX_OFFSCREEN_RECORDING_DIMENSION
            && (width * height) <= MAX_OFFSCREEN_RECORDING_AREA
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
    /// Concretely, the initial transformation matrix is constructed to:
    ///
    /// - scale the form/pattern `bbox` to exactly fit `recording_canvas` (independently in X/Y)
    /// - **not** apply a Y-axis flip
    /// - start with no translation; clipping is applied by `render_content_stream` when `bbox`
    ///   is provided
    ///
    /// This matches the expectation that the consumer of the resulting `RecordingCanvas`
    /// (e.g. a shader/mask client in the backend) also uses a top-left-origin coordinate system.
    ///
    /// # Parameters
    ///
    /// - `recording_canvas`: Target offscreen canvas to record into.
    /// - `content_stream`: The content stream containing the PDF operators to execute.
    /// - `mat`: Optional additional matrix (applied like a PDF `cm` / XObject `/Matrix`).
    /// - `bbox`: The content-space bounding box to map to the recording surface.
    /// - `resources`: Optional resource dictionary for resolving fonts, patterns, etc.
    /// - `filter`: Optional filter function to skip certain operations.
    ///
    /// # Errors
    ///
    /// Returns [`PdfCanvasError`] if rendering fails or the stream contains unsupported
    /// operations.
    pub(crate) fn record_content_stream(
        &self,
        recording_canvas: &mut RecordingCanvas,
        content_stream: &ContentStream,
        mat: Option<Transform>,
        bbox: &Rect,
        resources: Option<&'a Resources>,
        filter: Option<&mut (dyn FnMut(&PdfOperatorVariant) -> bool + '_)>,
    ) -> Result<(), PdfCanvasError> {
        // Calculate scale factors.
        let scale_x = recording_canvas.width() / bbox.width();
        let scale_y = recording_canvas.height() / bbox.height();

        // Directly construct the userspace transformation matrix.
        let transform = Transform::from_row(
            scale_x, // sx: Scale X
            0.0,     // ky: Skew Y (none)
            0.0,     // kx: Skew X (none)
            scale_y, // sy: Scale Y
            0.0,     // tx: Translate X (none)
            0.0,     // ty: Translate Y (none)
        );

        let canvas_stack = vec![CanvasState {
            transform,
            text_state: TextState::default(),
            ..Default::default()
        }];

        let mut other = PdfCanvas::<RecordingCanvas> {
            current_path: None,
            canvas: recording_canvas,
            mask: None,
            page: self.page,
            canvas_stack,
            active_content_stream_ids: self.active_content_stream_ids.clone(),
            text_sink: None,
        };

        // Render the form's content stream into the mask canvas.
        other.render_content_stream(content_stream, mat, Some(bbox), resources, filter)
    }

    /// Returns a reference to the current graphics state on the stack.
    ///
    /// # Errors
    ///
    /// Returns an error if the graphics state stack is empty.
    pub(crate) fn current_state(&self) -> Result<&CanvasState<'a>, PdfCanvasError> {
        self.canvas_stack
            .last()
            .ok_or(PdfCanvasError::EmptyGraphicsStateStack)
    }

    /// Returns a mutable reference to the current graphics state on the stack.
    ///
    /// # Errors
    ///
    /// Returns an error if the graphics state stack is empty.
    pub(crate) fn current_state_mut(&mut self) -> Result<&mut CanvasState<'a>, PdfCanvasError> {
        self.canvas_stack
            .last_mut()
            .ok_or(PdfCanvasError::EmptyGraphicsStateStack)
    }

    /// Builds a shader from a parsed shading definition.
    ///
    /// # Parameters
    ///
    /// - `shading`: The shading pattern definition.
    /// - `matrix`: Optional transformation matrix for the shading.
    ///
    /// # Returns
    ///
    /// An appropriate `Shader` if supported, or an error if not implemented.
    pub(crate) fn build_shading_shader<'b>(
        &mut self,
        shading: &'b Shading,
        transform: &Option<Transform>,
    ) -> Result<Shader<'b>, PdfCanvasError> {
        build_shading_paint(shading, *transform)
            .map(Shader::Shading)
            .map_err(|error| PdfCanvasError::UnsupportedFeature(error.to_string()))
    }

    /// Computes the current shader based on the active pattern.
    ///
    /// # Returns
    ///
    /// An optional `Shader` or an error if pattern rendering fails.
    fn compute_shader(&mut self, for_stroke: bool) -> Result<Option<Shader<'a>>, PdfCanvasError> {
        let state: &CanvasState<'_> = self.current_state()?;
        let pattern = if for_stroke {
            &state.stroke_pattern
        } else {
            &state.fill_pattern
        };

        let Some(pattern) = pattern else {
            return Ok(None);
        };

        match pattern {
            Pattern::Shading {
                shading, matrix, ..
            } => {
                let device_height = self.canvas.height();
                let mut shader_transform = Transform::from_row(
                    1.0,           // sx: keep X scale
                    0.0,           // ky: no skew
                    0.0,           // kx: no skew
                    -1.0,          // sy: flip Y (PDF up -> device down)
                    0.0,           // tx: no translation in X
                    device_height, // ty: translate after Y flip to keep content on-canvas
                );

                if let Some(pattern_matrix) = matrix {
                    shader_transform.post_concat(pattern_matrix);
                }

                let shader = self.build_shading_shader(shading, &Some(shader_transform))?;
                Ok(Some(shader))
            }
            Pattern::Tiling {
                bbox,
                resources,
                content_stream,
                matrix,
                x_step,
                y_step,
                paint_type,
                ..
            } => {
                let bbox = *bbox;
                if !Self::can_record_offscreen_bbox(&bbox) {
                    return Ok(None);
                }

                // The tiling pattern's `/Matrix` maps pattern space -> user space.
                // We pass it through unchanged and let the backend concatenate it with
                // the current CTM when sampling the pattern.
                let transform = *matrix;

                // Uncolored patterns use the current color from the graphics state,
                // so we filter out color-setting operators from the content stream.
                let mut uncolored_filter = |op: &PdfOperatorVariant| -> bool {
                    matches!(
                        op,
                        PdfOperatorVariant::SetNonStrokingColor(_)
                            | PdfOperatorVariant::SetStrokingColor(_)
                            | PdfOperatorVariant::SetGrayFill(_)
                            | PdfOperatorVariant::SetGrayStroke(_)
                            | PdfOperatorVariant::SetRGBFill(_)
                            | PdfOperatorVariant::SetRGBStroke(_)
                            | PdfOperatorVariant::SetCMYKFill(_)
                            | PdfOperatorVariant::SetCMYKStroke(_)
                    )
                };

                let filter: Option<&mut (dyn FnMut(&PdfOperatorVariant) -> bool)> = match paint_type
                {
                    PaintType::Colored => None,
                    PaintType::Uncolored => Some(&mut uncolored_filter),
                };

                // Create a recording canvas to render the tiling pattern.
                let mut recording_canvas = RecordingCanvas::new(bbox.width(), bbox.height());

                // Render the tiling content into a temporary canvas.
                self.record_content_stream(
                    &mut recording_canvas,
                    content_stream,
                    None,
                    &bbox,
                    Some(resources),
                    filter,
                )?;

                let shader = Shader::TilingPatternImage {
                    image: Arc::new(recording_canvas),
                    transform,
                    x_step: *x_step,
                    y_step: *y_step,
                };
                Ok(Some(shader))
            }
        }
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
        path: &PdfPath,
        mode: PaintMode,
        fill_type: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        let state = self.current_state()?;

        let fill_color = state.fill_color;
        let stroke_color = state.stroke_color;
        let blend_mode = state.blend_mode;
        let line_width = state.line_width * state.transform.sx;
        let stroke_style = StrokeStyle {
            dash_pattern: state.dash_pattern.clone(),
        }
        .scaled(state.transform.sx);

        match mode {
            PaintMode::Fill => {
                let shader = self.compute_shader(false)?;
                self.canvas
                    .fill_path(path, fill_type, fill_color, &shader, blend_mode)
            }
            PaintMode::Stroke => {
                let shader = self.compute_shader(true)?;
                self.canvas.stroke_path(
                    path,
                    stroke_color,
                    line_width,
                    &stroke_style,
                    &shader,
                    blend_mode,
                )
            }
            PaintMode::FillAndStroke => {
                // First fill the path using the current fill settings
                let fill_shader = self.compute_shader(false)?;
                self.canvas
                    .fill_path(path, fill_type, fill_color, &fill_shader, blend_mode)?;

                // Then stroke the path using the current stroke settings
                let stroke_shader = self.compute_shader(true)?;
                self.canvas.stroke_path(
                    path,
                    stroke_color,
                    line_width,
                    &stroke_style,
                    &stroke_shader,
                    blend_mode,
                )
            }
        }
    }

    /// Replays a path into the current graphics path.
    ///
    /// This is used by callers that need to materialize a [`PdfPath`] through the
    /// canvas path-construction API before painting it.
    pub fn replay_path(&mut self, path: &PdfPath) -> Result<(), PdfCanvasError> {
        for verb in &path.verbs {
            match *verb {
                PathVerb::MoveTo { x, y } => self.move_to(x, y)?,
                PathVerb::LineTo { x, y } => self.line_to(x, y)?,
                PathVerb::CubicTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x3,
                    y3,
                } => self.curve_to(x1, y1, x2, y2, x3, y3)?,
                PathVerb::QuadTo { .. } => {
                    return Err(PdfCanvasError::UnsupportedFeature(
                        "quadratic annotation paths".to_owned(),
                    ));
                }
                PathVerb::Close => self.close_path()?,
            }
        }
        Ok(())
    }

    /// Sets the alpha value for subsequent non-stroking paint operations.
    ///
    /// This updates only the fill alpha component in the current graphics state.
    pub fn set_non_stroking_alpha(&mut self, alpha: f32) -> Result<(), PdfCanvasError> {
        self.current_state_mut()?.fill_color.a = alpha;
        Ok(())
    }

    /// Sets the alpha value for subsequent stroking paint operations.
    ///
    /// This updates only the stroke alpha component in the current graphics state.
    pub fn set_stroking_alpha(&mut self, alpha: f32) -> Result<(), PdfCanvasError> {
        self.current_state_mut()?.stroke_color.a = alpha;
        Ok(())
    }

    /// Sets the blend mode for subsequent paint operations.
    pub fn set_blend_mode(&mut self, blend_mode: Option<BlendMode>) -> Result<(), PdfCanvasError> {
        self.current_state_mut()?.blend_mode = blend_mode;
        Ok(())
    }

    /// Paints the current path (if any) using the specified paint mode and fill type, then clears the path.
    ///
    /// # Parameters
    ///
    /// - `mode`: The paint mode (fill, stroke, or fill and stroke).
    /// - `fill_type`: The fill rule to use.
    ///
    /// # Errors
    ///
    /// Returns an error if there is no active path or if drawing fails.
    pub(crate) fn paint_taken_path(
        &mut self,
        mode: PaintMode,
        fill_type: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        let Some(mut path) = self.current_path.take() else {
            return Ok(());
        };
        path.transform(&self.current_state()?.transform);
        self.draw_path(&path, mode, fill_type)
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
        mut path: PdfPath,
        mode: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        path.transform(&self.current_state()?.transform);

        self.canvas.set_clip_region(&path, mode)?;
        self.current_state_mut()?.clip_path = Some(path);
        Ok(())
    }

    /// Appends a glyph outline (already in device space) to the pending text clip accumulator.
    ///
    /// Call this for every glyph rendered with a clip mode (4–7). The accumulated path
    /// is applied as a clip region when the text object ends via `end_text_object`.
    pub(crate) fn add_to_text_clip(&mut self, path: &PdfPath) -> Result<(), PdfCanvasError> {
        let state = self.current_state_mut()?;
        match &mut state.pending_text_clip {
            Some(clip) => clip.extend(path),
            slot => *slot = Some(path.clone()),
        }
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
    pub(crate) fn set_fill_pattern(&mut self, pattern_name: &str) -> Result<(), PdfCanvasError> {
        let Some(pattern) = self
            .current_state()?
            .resources
            .and_then(|r| r.pattern(pattern_name))
        else {
            return Err(PdfCanvasError::PatternNotFound(pattern_name.to_string()));
        };
        self.current_state_mut()?.fill_pattern = Some(pattern);
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
    pub(crate) fn set_stroke_pattern(&mut self, pattern_name: &str) -> Result<(), PdfCanvasError> {
        let Some(pattern) = self
            .current_state()?
            .resources
            .and_then(|r| r.pattern(pattern_name))
        else {
            return Err(PdfCanvasError::PatternNotFound(pattern_name.to_string()));
        };
        self.current_state_mut()?.stroke_pattern = Some(pattern);
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
        resources: Option<&'a Resources>,
        mut filter: Option<&mut (dyn FnMut(&PdfOperatorVariant) -> bool + '_)>,
    ) -> Result<(), PdfCanvasError> {
        if !self.active_content_stream_ids.insert(content_stream.id) {
            return Ok(());
        }

        self.save()?;

        if let Some(mat) = mat {
            // Concatenate the provided `XObject` matrix with the current CTM.
            // PDF spec: invoking a form XObject with its /Matrix entry performs a
            // concatenation like the 'cm' operator does. The operation is:
            //   CTM' = CTM * FormMatrix
            self.current_state_mut()?.transform.post_concat(&mat);
        }

        if let Some(bbox) = bbox {
            // Set up a clipping path based on the bounding box.
            let mut clip_path = PdfPath::default();
            clip_path.move_to(bbox.left, bbox.top);
            clip_path.line_to(bbox.right, bbox.top);
            clip_path.line_to(bbox.right, bbox.bottom);
            clip_path.line_to(bbox.left, bbox.bottom);
            clip_path.close();

            self.set_clip_path(clip_path, PathFillType::EvenOdd)?;
        }

        if let Some(resources) = resources {
            self.current_state_mut()?.resources = Some(resources);
        }

        for op in &content_stream.operators {
            if filter.as_mut().is_some_and(|filter| filter(op)) {
                continue;
            }
            op.call(self)?;
        }

        self.restore()?;

        let _ = self.active_content_stream_ids.remove(&content_stream.id);
        Ok(())
    }

    /// Saves the entire current graphics state onto a stack.
    ///
    /// This includes the current transformation matrix, colors, line styles, and clipping path.
    /// A corresponding call to `restore` is required to pop the state from the stack.
    pub(crate) fn save(&mut self) -> Result<(), PdfCanvasError> {
        let state = self.current_state()?.clone();
        self.canvas_stack.push(state);
        self.canvas.save()
    }

    /// Restores the most recently saved graphics state from the stack.
    ///
    /// If the restored state included a clipping path, the clipping path is reset on the backend.
    pub(crate) fn restore(&mut self) -> Result<(), PdfCanvasError> {
        // Do not allow popping the initial/base graphics state. There is no
        // corresponding backend `save()` for it, so treating this as an
        // underflow keeps the canvas stack and backend stack in sync.
        if self.canvas_stack.len() <= 1 {
            return Err(PdfCanvasError::GraphicsStateStackUnderflow);
        }

        // At this point there is at least one saved state beyond the base,
        // so popping is safe and has a matching backend `save()`.
        let _ = self.canvas_stack.pop();

        self.canvas.restore()
    }

    /// Sets the current color space for stroking or filling operations.
    ///
    /// # Parameters
    ///
    /// - `name`: The name of the color space to set.  c
    /// - `is_stroking`: If `true`, sets the stroking color space; otherwise, sets the filling color space.
    pub(crate) fn set_color_space(
        &mut self,
        name: &str,
        is_stroking: bool,
    ) -> Result<(), PdfCanvasError> {
        let state = self.current_state_mut()?;
        if is_stroking {
            state.stroke_pattern = None;
        } else {
            state.fill_pattern = None;
        }

        // The names /DeviceGray, /DeviceRGB, /DeviceCMYK, and /Pattern are reserved
        // keywords that always identify their corresponding colour spaces directly.
        // Per PDF spec §8.6.8 each also sets the current colour to its initial value.
        if matches!(name, "DeviceGray" | "DeviceRGB" | "DeviceCMYK" | "Pattern") {
            let (cs, initial_color) = match name {
                "DeviceGray" => (
                    &CanvasState::DEVICE_GRAY_COLOR_SPACE,
                    Some(Color::from_gray(0.0)),
                ),
                "DeviceRGB" => (
                    &CanvasState::DEVICE_RGB_COLOR_SPACE,
                    Some(Color::from_rgb(0.0, 0.0, 0.0)),
                ),
                "DeviceCMYK" => (
                    &CanvasState::DEVICE_CMYK_COLOR_SPACE,
                    Some(Color::from_cmyk(0.0, 0.0, 0.0, 1.0)),
                ),
                // Pattern: no initial colour is defined by the spec.
                _ => (&CanvasState::PATTERN_COLOR_SPACE, None),
            };
            if is_stroking {
                state.stroke_color_space = Some(cs);
                if let Some(color) = initial_color {
                    state.stroke_color = color;
                }
            } else {
                state.fill_color_space = Some(cs);
                if let Some(color) = initial_color {
                    state.fill_color = color;
                }
            }
            return Ok(());
        }

        let Some(cs) = state.resources.and_then(|res| res.color_space(name)) else {
            return Err(PdfCanvasError::ColorSpaceNotFound(name.to_string()));
        };

        if is_stroking {
            state.stroke_color_space = Some(cs);
        } else {
            state.fill_color_space = Some(cs);
        }
        Ok(())
    }

    /// Draws a font outline glyph onto the canvas using the current text rendering mode.
    ///
    /// # Parameters
    ///
    /// - `outline_glyph`: The outline representation of the glyph to render.
    /// - `transform`: The transformation matrix to apply to the glyph path
    ///   (maps glyph space to device space).
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the glyph is rendered successfully, or an error if drawing fails.
    pub(crate) fn draw_outline_glyph(
        &mut self,
        outline_glyph: &OutlineGlyph<'_>,
        transform: &Transform,
    ) -> Result<(), PdfCanvasError> {
        let mut pen = PdfPathPen::default();
        let settings = DrawSettings::from((Size::unscaled(), LocationRef::default()));
        if outline_glyph.draw(settings, &mut pen).is_err() {
            // Missing or malformed glyphs are not treated as errors: skip drawing this glyph.
            return Ok(());
        }

        pen.path.transform(transform);
        self.draw_glyph_path(&pen.path)
    }

    pub(crate) fn draw_glyph_path(&mut self, path: &PdfPath) -> Result<(), PdfCanvasError> {
        let rendering_mode = self.current_state()?.rendering_mode;
        if rendering_mode == TextRenderingMode::Invisible {
            return Ok(());
        }

        match rendering_mode {
            TextRenderingMode::Fill => self.draw_path(path, PaintMode::Fill, PathFillType::Winding),
            TextRenderingMode::Stroke => {
                self.draw_path(path, PaintMode::Stroke, PathFillType::Winding)
            }
            TextRenderingMode::FillAndStroke => {
                self.draw_path(path, PaintMode::FillAndStroke, PathFillType::Winding)
            }
            TextRenderingMode::Invisible => Ok(()),
            TextRenderingMode::FillClip => {
                self.draw_path(path, PaintMode::Fill, PathFillType::Winding)?;
                self.add_to_text_clip(path)
            }
            TextRenderingMode::StrokeClip => {
                self.draw_path(path, PaintMode::Stroke, PathFillType::Winding)?;
                self.add_to_text_clip(path)
            }
            TextRenderingMode::FillStrokeClip => {
                self.draw_path(path, PaintMode::FillAndStroke, PathFillType::Winding)?;
                self.add_to_text_clip(path)
            }
            TextRenderingMode::Clip => self.add_to_text_clip(path),
        }
    }

    pub(crate) fn record_text_glyph(
        &mut self,
        char_code: u16,
        text_state_before_advance: &TextState<'a>,
        ctm: &Transform,
    ) -> Result<(), PdfCanvasError> {
        if self.text_sink.is_none() {
            return Ok(());
        }

        let text_state_after_advance = &self.current_state()?.text_state;
        let text = glyph_text(text_state_before_advance.font, char_code);
        if text.is_empty() {
            return Ok(());
        }

        let bounds = glyph_bounds(text_state_before_advance, ctm, text_state_after_advance);
        if bounds.is_valid() {
            let Some(text_sink) = self.text_sink.as_deref_mut() else {
                return Ok(());
            };
            text_sink.push_glyph(TextGlyph { text, bounds });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, ops::Deref, rc::Rc, sync::Arc};

    use pdf_content_stream::ContentStreamIdAllocator;
    use pdf_content_stream_operators::pdf_operator_backend::GraphicsStateOps;
    use pdf_document::page::PdfPage;
    use pdf_graphics::{
        BlendMode, MaskMode, PaintMode, PathFillType, PixelFormat, color::Color, pdf_path::PdfPath,
        rect::Rect, transform::Transform,
    };
    use pdf_image::InlineImage;
    use pdf_object::{dictionary::Dictionary, object_variant::ObjectVariant, stream::StreamObject};
    use pdf_resources::{
        form::FormXObject, resource::Resource, resources::Resources, xobject::XObject,
    };

    use crate::{
        canvas_backend::{CanvasBackend, Image, Shader},
        recording_canvas::RecordingCanvas,
    };

    use super::PdfCanvas;

    #[derive(Default)]
    struct CountingCanvas {
        save_count: usize,
        restore_count: usize,
        draw_image_count: usize,
        draw_inline_image_count: usize,
        stroke_count: usize,
        last_stroke_style: Option<crate::stroke_style::StrokeStyle>,
        last_draw_image: Option<DrawnImage>,
        last_draw_inline_image: Option<DrawnImage>,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct DrawnImage {
        data: Vec<u8>,
        width: usize,
        height: usize,
        pixel_format: PixelFormat,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    }

    impl CanvasBackend for CountingCanvas {
        fn fill_path(
            &mut self,
            _path: &pdf_graphics::pdf_path::PdfPath,
            _fill_type: PathFillType,
            _color: Color,
            _shader: &Option<Shader>,
            _blend_mode: Option<BlendMode>,
        ) -> Result<(), crate::error::PdfCanvasError> {
            Ok(())
        }

        fn stroke_path(
            &mut self,
            _path: &pdf_graphics::pdf_path::PdfPath,
            _color: Color,
            _line_width: f32,
            stroke_style: &crate::stroke_style::StrokeStyle,
            _shader: &Option<Shader>,
            _blend_mode: Option<BlendMode>,
        ) -> Result<(), crate::error::PdfCanvasError> {
            self.stroke_count += 1;
            self.last_stroke_style = Some(stroke_style.clone());
            Ok(())
        }

        fn set_clip_region(
            &mut self,
            _path: &pdf_graphics::pdf_path::PdfPath,
            _mode: PathFillType,
        ) -> Result<(), crate::error::PdfCanvasError> {
            Ok(())
        }

        fn width(&self) -> f32 {
            100.0
        }

        fn height(&self) -> f32 {
            100.0
        }

        fn save(&mut self) -> Result<(), crate::error::PdfCanvasError> {
            self.save_count += 1;
            Ok(())
        }

        fn restore(&mut self) -> Result<(), crate::error::PdfCanvasError> {
            self.restore_count += 1;
            Ok(())
        }

        fn draw_image_rect(
            &mut self,
            _image: &Image<'_>,
            blend_mode: Option<BlendMode>,
            dest_rect: Rect,
            image_rotation: Option<f32>,
        ) -> Result<(), crate::error::PdfCanvasError> {
            self.draw_image_count += 1;
            self.last_draw_image = Some(DrawnImage {
                data: _image.data.deref().to_vec(),
                width: _image.width,
                height: _image.height,
                pixel_format: _image.pixel_format,
                blend_mode,
                dest_rect,
                image_rotation,
            });
            Ok(())
        }

        fn draw_inline_image(
            &mut self,
            image: &Image<'_>,
            blend_mode: Option<BlendMode>,
            dest_rect: Rect,
            image_rotation: Option<f32>,
        ) -> Result<(), crate::error::PdfCanvasError> {
            self.draw_inline_image_count += 1;
            self.last_draw_inline_image = Some(DrawnImage {
                data: image.data.deref().to_vec(),
                width: image.width,
                height: image.height,
                pixel_format: image.pixel_format,
                blend_mode,
                dest_rect,
                image_rotation,
            });
            Ok(())
        }

        fn begin_mask_layer(
            &mut self,
            _mask: &Arc<RecordingCanvas>,
            _transform: &Transform,
            _mask_mode: MaskMode,
        ) -> Result<(), crate::error::PdfCanvasError> {
            Ok(())
        }

        fn end_mask_layer(
            &mut self,
            _mask: &Arc<RecordingCanvas>,
            _transform: &Transform,
            _mask_mode: MaskMode,
        ) -> Result<(), crate::error::PdfCanvasError> {
            Ok(())
        }
    }

    fn page() -> PdfPage {
        PdfPage {
            contents: None,
            annotations: None,
            media_box: None,
            resources: None,
            annotation_id_high_watermark: 0,
        }
    }

    fn stream_object(object_number: usize, data: &[u8]) -> StreamObject {
        StreamObject::new(
            object_number,
            0,
            Box::new(Dictionary::new(Default::default())),
            data.to_vec(),
        )
    }

    fn image_xobject_dictionary() -> Dictionary {
        Dictionary::new(std::collections::BTreeMap::from([
            ("BitsPerComponent".to_string(), ObjectVariant::Integer(1)),
            (
                "ColorSpace".to_string(),
                ObjectVariant::Name(b"DeviceGray".to_vec()),
            ),
            (
                "Decode".to_string(),
                ObjectVariant::Array(vec![ObjectVariant::Integer(1), ObjectVariant::Integer(0)]),
            ),
            ("Height".to_string(), ObjectVariant::Integer(1)),
            ("Width".to_string(), ObjectVariant::Integer(4)),
        ]))
    }

    fn inline_image() -> InlineImage {
        InlineImage::new(
            Dictionary::new(std::collections::BTreeMap::from([
                ("BPC".to_string(), ObjectVariant::Integer(1)),
                (
                    "CS".to_string(),
                    ObjectVariant::Name(b"DeviceGray".to_vec()),
                ),
                (
                    "D".to_string(),
                    ObjectVariant::Array(vec![
                        ObjectVariant::Integer(1),
                        ObjectVariant::Integer(0),
                    ]),
                ),
                ("H".to_string(), ObjectVariant::Integer(1)),
                ("W".to_string(), ObjectVariant::Integer(4)),
            ])),
            vec![0b1010_0000],
        )
    }

    #[test]
    fn set_dash_pattern_updates_current_state() {
        let page = page();
        let mut backend = CountingCanvas::default();
        let mut canvas = PdfCanvas::new(&mut backend, &page, None).expect("canvas should build");

        canvas
            .set_dash_pattern(&[4.0, 2.0], 1.0)
            .expect("dash pattern should be valid");

        let dash_pattern = canvas
            .current_state()
            .expect("state should exist")
            .dash_pattern
            .as_ref()
            .expect("dash pattern should be stored");
        assert_eq!(dash_pattern.intervals, vec![4.0, 2.0]);
        assert_eq!(dash_pattern.phase, 1.0);
    }

    #[test]
    fn empty_dash_pattern_clears_current_state() {
        let page = page();
        let mut backend = CountingCanvas::default();
        let mut canvas = PdfCanvas::new(&mut backend, &page, None).expect("canvas should build");

        canvas
            .set_dash_pattern(&[4.0, 2.0], 1.0)
            .expect("dash pattern should be valid");
        canvas
            .set_dash_pattern(&[], 3.0)
            .expect("empty dash pattern should be valid");

        assert!(
            canvas
                .current_state()
                .expect("state should exist")
                .dash_pattern
                .is_none()
        );
    }

    #[test]
    fn invalid_dash_pattern_does_not_mutate_current_state() {
        let page = page();
        let mut backend = CountingCanvas::default();
        let mut canvas = PdfCanvas::new(&mut backend, &page, None).expect("canvas should build");

        canvas
            .set_dash_pattern(&[4.0, 2.0], 1.0)
            .expect("dash pattern should be valid");
        let result = canvas.set_dash_pattern(&[0.0, 0.0], 0.0);

        assert!(result.is_err());
        let dash_pattern = canvas
            .current_state()
            .expect("state should exist")
            .dash_pattern
            .as_ref()
            .expect("previous dash pattern should remain");
        assert_eq!(dash_pattern.intervals, vec![4.0, 2.0]);
        assert_eq!(dash_pattern.phase, 1.0);
    }

    #[test]
    fn draw_path_forwards_dash_pattern_to_backend() {
        let page = page();
        let mut backend = CountingCanvas::default();
        let mut canvas = PdfCanvas::new(&mut backend, &page, None).expect("canvas should build");
        let mut path = PdfPath::default();
        path.move_to(0.0, 0.0);
        path.line_to(10.0, 0.0);

        canvas
            .set_dash_pattern(&[4.0, 2.0], 1.0)
            .expect("dash pattern should be valid");
        canvas
            .draw_path(&path, PaintMode::Stroke, PathFillType::Winding)
            .expect("stroke should draw");
        drop(canvas);

        assert_eq!(backend.stroke_count, 1);
        let stroke_style = backend
            .last_stroke_style
            .expect("backend should receive stroke style");
        let dash_pattern = stroke_style
            .dash_pattern
            .expect("backend should receive dash pattern");
        assert_eq!(dash_pattern.intervals, vec![4.0, 2.0]);
        assert_eq!(dash_pattern.phase, 1.0);
    }

    fn form_resource(name: &str, content_stream: pdf_content_stream::ContentStream) -> Resources {
        Resources {
            xobjects: HashMap::from([(
                name.to_string(),
                Resource::XObject(Rc::new(XObject::Form(Box::new(FormXObject {
                    bbox: Rect {
                        left: 0.0,
                        top: 0.0,
                        right: 10.0,
                        bottom: 10.0,
                    },
                    matrix: None,
                    resources: None,
                    content_stream,
                })))),
            )]),
            ..Default::default()
        }
    }

    #[test]
    fn skips_recursive_render_when_content_stream_id_is_already_active() {
        let mut ids = ContentStreamIdAllocator::new();
        let root_stream = stream_object(1, b"/Self Do");
        let root = pdf_content_stream::ContentStream::new(
            &ObjectVariant::Stream(root_stream.clone()),
            &pdf_object::object_resolver::PassthroughResolver,
            &mut ids,
        )
        .expect("root stream should parse");
        let resources = form_resource(
            "Self",
            pdf_content_stream::ContentStream {
                operators: root.operators.clone(),
                id: root.id,
            },
        );

        let page = page();
        let mut backend = CountingCanvas::default();
        let mut canvas = PdfCanvas::new(&mut backend, &page, None).expect("canvas should build");

        canvas
            .render_content_stream(&root, None, None, Some(&resources), None)
            .expect("recursive render should be skipped gracefully");

        assert!(canvas.active_content_stream_ids.is_empty());
        assert_eq!(canvas.canvas_stack.len(), 1);
        assert_eq!(backend.save_count, 1);
        assert_eq!(backend.restore_count, 1);
    }

    #[test]
    fn still_renders_nested_streams_with_distinct_ids() {
        let mut ids = ContentStreamIdAllocator::new();
        let root_stream = stream_object(1, b"/Child Do");
        let child_stream = stream_object(2, b"q Q");
        let root = pdf_content_stream::ContentStream::new(
            &ObjectVariant::Stream(root_stream.clone()),
            &pdf_object::object_resolver::PassthroughResolver,
            &mut ids,
        )
        .expect("root stream should parse");
        let child = pdf_content_stream::ContentStream::new(
            &ObjectVariant::Stream(child_stream.clone()),
            &pdf_object::object_resolver::PassthroughResolver,
            &mut ids,
        )
        .expect("child stream should parse");
        let resources = form_resource("Child", child);

        let page = page();
        let mut backend = CountingCanvas::default();
        let mut canvas = PdfCanvas::new(&mut backend, &page, None).expect("canvas should build");

        canvas
            .render_content_stream(&root, None, None, Some(&resources), None)
            .expect("distinct nested stream should render");

        assert!(canvas.active_content_stream_ids.is_empty());
        assert_eq!(canvas.canvas_stack.len(), 1);
        assert_eq!(backend.save_count, 3);
        assert_eq!(backend.restore_count, 3);
    }

    #[test]
    fn inline_image_render_path_matches_image_xobject_path() {
        let page = page();
        let mut xobject_backend = CountingCanvas::default();
        let mut inline_backend = CountingCanvas::default();

        let transform = Transform::from_row(2.0, 0.0, 0.0, 3.0, 10.0, 20.0);

        let mut xobject_canvas =
            PdfCanvas::new(&mut xobject_backend, &page, None).expect("xobject canvas should build");
        xobject_canvas.current_state_mut().expect("state").transform = transform;
        xobject_canvas
            .current_state_mut()
            .expect("state")
            .blend_mode = Some(BlendMode::Multiply);

        let image = pdf_image::ImageXObject::decode_normalized_image(
            &image_xobject_dictionary(),
            &[0b1010_0000],
            &pdf_object::object_resolver::PassthroughResolver,
            None,
        )
        .expect("xobject image should decode");

        xobject_canvas
            .render_image_xobject(&image)
            .expect("xobject image should render");

        let mut inline_canvas =
            PdfCanvas::new(&mut inline_backend, &page, None).expect("inline canvas should build");
        inline_canvas.current_state_mut().expect("state").transform = transform;
        inline_canvas.current_state_mut().expect("state").blend_mode = Some(BlendMode::Multiply);

        pdf_content_stream_operators::pdf_operator_backend::XObjectOps::paint_inline_image(
            &mut inline_canvas,
            &inline_image(),
        )
        .expect("inline image should render");

        assert_eq!(xobject_backend.draw_image_count, 1);
        assert_eq!(inline_backend.draw_inline_image_count, 1);

        let xobject_draw = xobject_backend
            .last_draw_image
            .as_ref()
            .expect("xobject draw should be recorded");
        let inline_draw = inline_backend
            .last_draw_inline_image
            .as_ref()
            .expect("inline draw should be recorded");

        assert_eq!(xobject_draw, inline_draw);
        assert_eq!(xobject_draw.width, 4);
        assert_eq!(xobject_draw.height, 1);
        assert_eq!(xobject_draw.pixel_format, PixelFormat::Gray8);
        assert_eq!(xobject_draw.blend_mode, Some(BlendMode::Multiply));
        assert_eq!(xobject_draw.dest_rect, inline_draw.dest_rect);
        assert_eq!(xobject_draw.data, inline_draw.data);
        assert_eq!(xobject_draw.data, vec![0x00, 0xFF, 0x00, 0xFF]);
    }

    #[test]
    fn rejects_absurd_offscreen_recording_bbox() {
        assert!(!PdfCanvas::<CountingCanvas>::can_record_offscreen_bbox(
            &Rect {
                left: -32768.0,
                top: -32768.0,
                right: 32767.0,
                bottom: 32767.0,
            }
        ));
    }
}
