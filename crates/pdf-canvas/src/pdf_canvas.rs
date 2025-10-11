use num_traits::FromPrimitive;
use pdf_content_stream::pdf_operator::PdfOperatorVariant;
use pdf_graphics::{MaskMode, PaintMode, PathFillType, pdf_path::PdfPath, transform::Transform};
use pdf_page::{page::PdfPage, pattern::Pattern, resources::Resources, shading::Shading};

use crate::{
    canvas::Canvas,
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
    pub(crate) mask: Option<(Box<RecordingCanvas>, MaskMode)>,
    /// The PDF page associated with this canvas.
    pub(crate) page: &'a PdfPage,
    /// The stack of graphics states, supporting save/restore semantics.
    pub(crate) canvas_stack: Vec<CanvasState<'a>>,
}

impl<T: std::error::Error> Canvas for PdfCanvas<'_, T> {
    fn save(&mut self) -> Result<(), PdfCanvasError> {
        let mut state = self.current_state()?.clone();
        state.clip_path = None;

        self.canvas_stack.push(state);
        Ok(())
    }

    fn restore(&mut self) -> Result<(), PdfCanvasError> {
        let prev = self.canvas_stack.pop();
        if let Some(state) = prev
            && state.clip_path.is_some()
        {
            self.canvas
                .reset_clip()
                .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;
        }
        Ok(())
    }

    fn set_matrix(&mut self, matrix: Transform) -> Result<(), PdfCanvasError> {
        self.current_state_mut()?.transform = matrix;
        Ok(())
    }

    fn fill_path(&mut self, path: &PdfPath, fill_type: PathFillType) -> Result<(), PdfCanvasError> {
        self.draw_path(path, PaintMode::Fill, fill_type)
    }
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
        bb: Option<&[f32; 4]>,
    ) -> Result<Self, PdfCanvasError> {
        let media_box = &page.media_box;

        let (pdf_media_width, pdf_media_height) = if let Some(bb) = bb {
            (bb[2] - bb[0], bb[3] - bb[1])
        } else if let Some(mb) = media_box.as_ref() {
            (
                f32::from_u32(mb.width())
                    .ok_or(PdfCanvasError::NumericConversionError("u32 to f32 width"))?,
                f32::from_u32(mb.height())
                    .ok_or(PdfCanvasError::NumericConversionError("u32 to f32 height"))?,
            )
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
        let userspace_matrix = Transform::from_row(
            scale_x,               // sx: Scale X
            0.0,                   // ky: Skew Y (none)
            0.0,                   // kx: Skew X (none)
            -scale_y,              // sy: Scale Y and reflect (Y points down)
            0.0,                   // tx: Translate X (none)
            backend_canvas_height, // ty: Translate Y to move origin to top-left after reflection
        );

        let canvas_stack = vec![CanvasState {
            transform: userspace_matrix,
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
    /// Returns `None` when the shading type isn't yet supported or not applicable.
    /// Builds a `Shader` from a PDF shading pattern definition (Axial, Radial, or FunctionBased).
    ///
    /// # Parameters
    ///
    /// - `shading`: The shading pattern definition.
    /// - `matrix`: Optional transformation matrix for the shading.
    ///
    /// # Returns
    ///
    /// An appropriate `Shader` if supported, or an error if not implemented.
    fn build_shading_shader<'b>(
        &mut self,
        shading: &'b Shading,
        matrix: &Option<Transform>,
    ) -> Result<Shader<'b>, PdfCanvasError> {
        match shading {
            Shading::Axial {
                coords: [x0, y0, x1, y1],
                positions,
                colors,
                ..
            } => Ok(Shader::LinearGradient {
                x0: *x0,
                y0: *y0,
                x1: *x1,
                y1: *y1,
                colors,
                positions,
            }),
            Shading::Radial {
                coords: [start_x, start_y, start_r, end_x, end_y, end_r],
                positions,
                colors,
                ..
            } => {
                let transform = matrix.map(|mut mat| {
                    mat.ty = self.canvas.height() - mat.ty;
                    mat
                });
                Ok(Shader::RadialGradient {
                    start_x: *start_x,
                    start_y: *start_y,
                    start_r: *start_r,
                    end_x: *end_x,
                    end_y: *end_y,
                    end_r: *end_r,
                    transform,
                    colors,
                    positions,
                })
            }
            Shading::FunctionBased { .. } => Err(PdfCanvasError::NotImplemented(
                "FunctionBased shading not implemented".into(),
            )),
        }
    }

    /// Computes the current shader based on the active pattern.
    ///
    /// # Returns
    ///
    /// An optional `Shader` or an error if pattern rendering fails.
    fn compute_shader(&mut self) -> Result<Option<Shader<'a>>, PdfCanvasError> {
        let Some(pattern) = self.current_state()?.pattern else {
            return Ok(None);
        };

        match pattern {
            Pattern::Shading {
                shading, matrix, ..
            } => {
                let shader = self.build_shading_shader(shading, matrix)?;
                Ok(Some(shader))
            }
            Pattern::Tiling {
                bbox,
                resources,
                content_stream,
                ..
            } => {
                // Create a recording canvas to render the tiling pattern.
                let mut recording_canvas =
                    RecordingCanvas::new(bbox[2] - bbox[0], bbox[3] - bbox[1]);

                // Render the tiling content into a temporary canvas.
                let mut other = PdfCanvas::new(&mut recording_canvas, self.page, Some(bbox))?;
                other.render_content_stream(&content_stream.operations, None, Some(resources))?;
                let shader = Shader::TilingPatternImage {
                    image: Box::new(recording_canvas),
                    transform: None,
                    x_step: bbox[2] - bbox[0],
                    y_step: bbox[3] - bbox[1],
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
    fn draw_path(
        &mut self,
        path: &PdfPath,
        mode: PaintMode,
        fill_type: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        let shader = self.compute_shader()?;

        match mode {
            PaintMode::Fill => {
                self.canvas
                    .fill_path(
                        path,
                        fill_type,
                        self.current_state()?.fill_color,
                        &shader,
                        self.current_state()?.blend_mode,
                    )
                    .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;
            }
            PaintMode::Stroke => {
                self.canvas
                    .stroke_path(
                        path,
                        self.current_state()?.stroke_color,
                        self.current_state()?.line_width,
                        &shader,
                        self.current_state()?.blend_mode,
                    )
                    .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;
            }
            PaintMode::FillAndStroke => {
                return Err(PdfCanvasError::NotImplemented(
                    "FillAndStroke mode is not implemented".into(),
                ));
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
            return Err(PdfCanvasError::NoActivePath);
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
        if self.current_state()?.clip_path.is_some() {
            self.canvas
                .reset_clip()
                .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;
        }

        self.canvas
            .set_clip_region(&path, mode)
            .map_err(|e| PdfCanvasError::BackendError(e.to_string()))?;
        self.current_state_mut()?.clip_path = Some(path);
        Ok(())
    }

    /// Sets the current pattern by name from the page resources.
    ///
    /// # Parameters
    ///
    /// - `pattern_name`: The name of the pattern to activate.
    ///
    /// # Errors
    ///
    /// Returns an error if the pattern is not found in the resources.
    pub(crate) fn set_pattern(&mut self, pattern_name: &str) -> Result<(), PdfCanvasError> {
        let Some(pattern) = self
            .page
            .resources
            .as_ref()
            .and_then(|r| r.patterns.get(pattern_name))
        else {
            if let Some(pattern) = self
                .current_state()?
                .resources
                .and_then(|r| r.patterns.get(pattern_name))
            {
                self.current_state_mut()?.pattern = Some(pattern);
                return Ok(());
            }
            return Err(PdfCanvasError::PatternNotFound(pattern_name.to_string()));
        };

        self.current_state_mut()?.pattern = Some(pattern);
        Ok(())
    }

    /// Returns the current resource dictionary, or the page's resources if not overridden.
    ///
    /// # Errors
    ///
    /// Returns an error if no resources are available.
    pub(crate) fn get_resources(&self) -> Result<&'a Resources, PdfCanvasError> {
        if let Some(resources) = self.current_state()?.resources {
            Ok(resources)
        } else {
            self.page
                .resources
                .as_ref()
                .ok_or(PdfCanvasError::MissingPageResources)
        }
    }

    /// Renders a sequence of PDF content stream operations onto the canvas.
    ///
    /// # Parameters
    ///
    /// - `operations`: The list of PDF operators to execute.
    /// - `mat`: Optional transformation matrix to apply.
    /// - `resources`: Optional resource dictionary to use for rendering.
    ///
    /// # Errors
    ///
    /// Returns an error if any operation fails or if the graphics state is invalid.
    pub fn render_content_stream(
        &mut self,
        operations: &[PdfOperatorVariant],
        mat: Option<Transform>,
        resources: Option<&'a Resources>,
    ) -> Result<(), PdfCanvasError> {
        self.save()?;

        if let Some(mat) = mat {
            // Concatenate the provided Form/XObject matrix with the current CTM.
            // PDF spec: invoking a form XObject with its /Matrix entry performs a
            // concatenation like the 'cm' operator does. The operation is:
            //   CTM' = FormMatrix * CTM
            // Our Transform::concat(other) implements self = other * self (pre-multiply),
            // so we just call concat with the form matrix.
            self.current_state_mut()?.transform.concat(&mat);
        }

        if let Some(resources) = resources {
            self.current_state_mut()?.resources = Some(resources);
        }

        for op in operations {
            op.call(self)?;
        }

        self.restore()
    }
}
