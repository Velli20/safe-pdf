use std::borrow::Cow;

use pdf_content_stream::pdf_operator::PdfOperatorVariant;
use pdf_graphics::{
    MaskMode, PaintMode, PathFillType, pdf_path::PdfPath, rect::Rect, transform::Transform,
};
use pdf_page::{
    page::PdfPage,
    pattern::{PaintType, Pattern},
    resources::Resources,
    shading::Shading,
};

use crate::{
    canvas_backend::{CanvasBackend, Shader},
    canvas_state::CanvasState,
    error::PdfCanvasError,
    recording_canvas::RecordingCanvas,
    text_state::TextState,
};

pub struct PdfCanvas<'a, T> {
    /// The current path being constructed or drawn, if any.
    pub(crate) current_path: Option<PdfPath>,
    /// The drawing backend implementing `CanvasBackend` for rendering operations.
    pub(crate) canvas: &'a mut dyn CanvasBackend<ErrorType = T>,
    /// An optional mask surface for advanced compositing or clipping.
    pub(crate) mask: Option<(Box<RecordingCanvas>, MaskMode, Transform)>,
    /// The PDF page associated with this canvas.
    pub(crate) page: &'a PdfPage,
    /// The stack of graphics states, supporting save/restore semantics.
    pub(crate) canvas_stack: Vec<CanvasState<'a>>,
}

impl<'a, T: std::error::Error> PdfCanvas<'a, T>
where
    T: 'a,
{
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
        backend: &'a mut dyn CanvasBackend<ErrorType = T>,
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
        })
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
    /// - `operations`: Parsed operator list to execute.
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
        operations: &[PdfOperatorVariant],
        mat: Option<Transform>,
        bbox: &Rect,
        resources: Option<&'a Resources>,
        filter: Option<&(dyn Fn(&PdfOperatorVariant) -> bool + '_)>,
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

        let mut other = PdfCanvas {
            current_path: None,
            canvas: recording_canvas,
            mask: None,
            page: self.page,
            canvas_stack,
        };

        // Render the form's content stream into the mask canvas.
        other.render_content_stream(operations, mat, Some(bbox), resources, filter)
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

    /// Builds a shader from a shading pattern definition (Axial / Radial / FunctionBased).
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
        match shading {
            Shading::Axial {
                coords: [x0, y0, x1, y1],
                color_stops,
                ..
            } => Ok(Shader::LinearGradient {
                x0: *x0,
                y0: *y0,
                x1: *x1,
                y1: *y1,
                colors: Cow::Borrowed(&color_stops.colors),
                transform: *transform,
                positions: Cow::Borrowed(&color_stops.positions),
            }),
            Shading::Radial {
                coords: [start_x, start_y, start_r, end_x, end_y, end_r],
                color_stops,
                ..
            } => Ok(Shader::RadialGradient {
                start_x: *start_x,
                start_y: *start_y,
                start_r: *start_r,
                end_x: *end_x,
                end_y: *end_y,
                end_r: *end_r,
                transform: *transform,
                colors: Cow::Borrowed(&color_stops.colors),
                positions: Cow::Borrowed(&color_stops.positions),
            }),
            Shading::FunctionBased { .. } => Err(PdfCanvasError::NotImplemented(
                "FunctionBased shading not implemented".into(),
            )),
            Shading::Unsupported { name } => Err(PdfCanvasError::NotImplemented(format!(
                "Shading type '{}' not implemented",
                name
            ))),
        }
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

                // The tiling pattern's `/Matrix` maps pattern space -> user space.
                // We pass it through unchanged and let the backend concatenate it with
                // the current CTM when sampling the pattern.
                let transform = *matrix;

                // Uncolored patterns use the current color from the graphics state,
                // so we filter out color-setting operators from the content stream.
                const UNCOLORED_FILTER: fn(&PdfOperatorVariant) -> bool = |op| {
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

                let filter: Option<&dyn Fn(&PdfOperatorVariant) -> bool> = match paint_type {
                    PaintType::Colored => None,
                    PaintType::Uncolored => Some(&UNCOLORED_FILTER),
                };

                // Create a recording canvas to render the tiling pattern.
                let mut recording_canvas = RecordingCanvas::new(bbox.width(), bbox.height());

                // Render the tiling content into a temporary canvas.
                self.record_content_stream(
                    &mut recording_canvas,
                    &content_stream.operations,
                    None,
                    &bbox,
                    Some(resources),
                    filter,
                )?;

                let shader = Shader::TilingPatternImage {
                    image: Box::new(recording_canvas),
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

        match mode {
            PaintMode::Fill => {
                let shader = self.compute_shader(false)?;
                self.canvas
                    .fill_path(path, fill_type, fill_color, &shader, blend_mode)
                    .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;
            }
            PaintMode::Stroke => {
                let shader = self.compute_shader(true)?;
                self.canvas
                    .stroke_path(path, stroke_color, line_width, &shader, blend_mode)
                    .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;
            }
            PaintMode::FillAndStroke => {
                // First fill the path using the current fill settings
                let fill_shader = self.compute_shader(false)?;
                self.canvas
                    .fill_path(path, fill_type, fill_color, &fill_shader, blend_mode)
                    .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;

                // Then stroke the path using the current stroke settings
                let stroke_shader = self.compute_shader(true)?;
                self.canvas
                    .stroke_path(path, stroke_color, line_width, &stroke_shader, blend_mode)
                    .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;
            }
        }
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

        self.canvas
            .set_clip_region(&path, mode)
            .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;
        self.current_state_mut()?.clip_path = Some(path);
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
            .and_then(|r| r.patterns.get(pattern_name))
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
            .and_then(|r| r.patterns.get(pattern_name))
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
    /// - `operations`: The list of PDF operators to execute.
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
        operations: &[PdfOperatorVariant],
        mat: Option<Transform>,
        bbox: Option<&Rect>,
        resources: Option<&'a Resources>,
        filter: Option<&(dyn Fn(&PdfOperatorVariant) -> bool + '_)>,
    ) -> Result<(), PdfCanvasError> {
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

        for op in operations {
            if filter.is_some_and(|filter| filter(op)) {
                continue;
            }
            op.call(self)?;
        }

        self.restore()
    }

    /// Saves the entire current graphics state onto a stack.
    ///
    /// This includes the current transformation matrix, colors, line styles, and clipping path.
    /// A corresponding call to `restore` is required to pop the state from the stack.
    pub(crate) fn save(&mut self) -> Result<(), PdfCanvasError> {
        let state = self.current_state()?.clone();
        self.canvas_stack.push(state);
        self.canvas
            .save()
            .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;
        Ok(())
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

        self.canvas
            .restore()
            .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;
        Ok(())
    }

    /// Replaces the current transformation matrix (CTM) with the given matrix.
    ///
    /// This sets the complete transformation from user space to device space.
    pub(crate) fn set_matrix(&mut self, matrix: Transform) -> Result<(), PdfCanvasError> {
        self.current_state_mut()?.transform = matrix;
        Ok(())
    }
}
