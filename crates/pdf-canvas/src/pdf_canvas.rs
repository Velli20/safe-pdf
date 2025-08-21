use pdf_content_stream::{
    graphics_state_operators::{LineCap, LineJoin},
    pdf_operator::PdfOperatorVariant,
};
use pdf_font::font::Font;
use pdf_graphics::{color::Color, transform::Transform};
use pdf_page::{page::PdfPage, pattern::Pattern, resources::Resources};

use crate::{
    PaintMode, PathFillType,
    canvas::Canvas,
    canvas_backend::{CanvasBackend, Shader},
    error::PdfCanvasError,
    pdf_path::PdfPath,
};

/// Encapsulates text-specific state parameters.
#[derive(Clone)]
pub(crate) struct TextState<'a> {
    /// The text matrix (Tm), transforming text space to user space.
    pub(crate) matrix: Transform,
    /// The text line matrix (Tlm), tracking the start of the current line.
    pub(crate) line_matrix: Transform,
    /// Horizontal scaling of text (Th), as a percentage (default: 100.0).
    pub(crate) horizontal_scaling: f32,
    /// Font size (Tfs), in user space units.
    pub(crate) font_size: f32,
    /// Character spacing (Tc), in unscaled text space units.
    pub(crate) character_spacing: f32,
    /// Word spacing (Tw), in unscaled text space units.
    pub(crate) word_spacing: f32,
    /// Text rise (Ts), a vertical offset from the baseline, in unscaled text space units.
    pub(crate) rise: f32,
    /// The current font resource.
    pub(crate) font: Option<&'a Font>,
}

impl Default for TextState<'_> {
    fn default() -> Self {
        Self {
            matrix: Transform::identity(),
            line_matrix: Transform::identity(),
            horizontal_scaling: 100.0,
            font_size: 0.0,
            character_spacing: 0.0,
            word_spacing: 0.0,
            rise: 0.0,
            font: None,
        }
    }
}

#[derive(Clone)]
pub(crate) struct CanvasState<'a> {
    pub transform: Transform,
    pub stroke_color: Color,
    pub fill_color: Color,
    pub line_width: f32,
    pub miter_limit: f32,
    pub text_state: TextState<'a>,
    pub clip_path: Option<PdfPath>,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    /// The current font resource.
    pub resources: Option<&'a Resources>,
    pub pattern: Option<&'a Pattern>,
}

impl CanvasState<'_> {
    /// Default line width in user space units.
    const DEFAULT_LINE_WIDTH: f32 = 1.0;
    /// Default fill color.
    const DEFAULT_FILL_COLOR: Color = Color::from_rgb(0.0, 0.0, 0.0);
    /// Default stroke color.
    const DEFAULT_STROKE_COLOR: Color = Color::from_rgb(0.0, 0.0, 0.0);
    /// Default miter limit.
    const DEFAULT_MITER_LIMIT: f32 = 0.0;
}

impl Default for CanvasState<'_> {
    fn default() -> Self {
        Self {
            transform: Transform::identity(),
            stroke_color: Self::DEFAULT_STROKE_COLOR,
            fill_color: Self::DEFAULT_FILL_COLOR,
            line_width: Self::DEFAULT_LINE_WIDTH,
            miter_limit: Self::DEFAULT_MITER_LIMIT,
            text_state: TextState::default(),
            clip_path: None,
            resources: None,
            pattern: None,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
        }
    }
}

pub struct PdfCanvas<'a, T, U> {
    pub(crate) current_path: Option<PdfPath>,
    pub(crate) canvas: &'a mut dyn CanvasBackend<MaskType = T, ImageType = U>,
    pub(crate) mask: Option<Box<T>>,
    pub(crate) page: &'a PdfPage,
    // Stores the graphics states, including text state.
    pub(crate) canvas_stack: Vec<CanvasState<'a>>,
}

impl<U, T: CanvasBackend<ImageType = U>> Canvas for PdfCanvas<'_, T, U> {
    fn save(&mut self) -> Result<(), PdfCanvasError> {
        let mut state = self.current_state()?.clone();
        state.clip_path = None;

        self.canvas_stack.push(state);
        Ok(())
    }

    fn restore(&mut self) -> Result<(), PdfCanvasError> {
        let prev = self.canvas_stack.pop();
        if let Some(state) = prev {
            if state.clip_path.is_some() {
                self.canvas.reset_clip();
            }
        }
        Ok(())
    }

    fn set_matrix(&mut self, matrix: Transform) -> Result<(), PdfCanvasError> {
        self.current_state_mut()?.transform = matrix;
        Ok(())
    }

    fn fill_path(&mut self, path: &PdfPath, fill_type: PathFillType) -> Result<(), PdfCanvasError> {
        if self.current_state()?.pattern.is_some() {
            println!("Pattern fill is not supported yet.");
        }
        let fill_color = self.current_state()?.fill_color;
        self.canvas
            .fill_path(path, fill_type, fill_color, &None, None);
        Ok(())
    }
}

impl<'a, U, T: CanvasBackend<ImageType = U>> PdfCanvas<'a, T, U>
where
    T: 'a,
{
    pub fn new(
        backend: &'a mut dyn CanvasBackend<MaskType = T, ImageType = U>,
        page: &'a PdfPage,
        bb: Option<&[f32; 4]>,
    ) -> Self {
        let media_box = &page.media_box;

        let pdf_media_width = if let Some(bb) = bb {
            bb[2] - bb[0]
        } else {
            #[allow(clippy::as_conversions)]
            match *media_box {
                Some(ref mb) => mb.width() as f32,
                None => 0.0,
            }
        };
        let pdf_media_height = if let Some(bb) = bb {
            bb[3] - bb[1]
        } else {
            #[allow(clippy::as_conversions)]
            match *media_box {
                Some(ref mb) => mb.height() as f32,
                None => 0.0,
            }
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

        Self {
            current_path: None,
            canvas: backend,
            mask: None,
            page,
            canvas_stack,
        }
    }

    /// Returns a reference to the current graphics state.
    pub(crate) fn current_state(&self) -> Result<&CanvasState<'a>, PdfCanvasError> {
        self.canvas_stack
            .last()
            .ok_or(PdfCanvasError::EmptyGraphicsStateStack)
    }

    /// Returns a mutable reference to the current graphics state.
    pub(crate) fn current_state_mut(&mut self) -> Result<&mut CanvasState<'a>, PdfCanvasError> {
        self.canvas_stack
            .last_mut()
            .ok_or(PdfCanvasError::EmptyGraphicsStateStack)
    }

    /// Builds a shader from a shading pattern definition (Axial / Radial / FunctionBased).
    /// Returns `None` when the shading type isn't yet supported or not applicable.
    fn build_shading_shader(
        &mut self,
        shading: &pdf_page::shading::Shading,
        matrix: &Option<pdf_graphics::transform::Transform>,
    ) -> Option<Shader> {
        use pdf_page::shading::Shading;

        match shading {
            Shading::Axial {
                coords: [x0, y0, x1, y1],
                positions,
                colors,
                ..
            } => Some(Shader::LinearGradient {
                x0: *x0,
                y0: *y0,
                x1: *x1,
                y1: *y1,
                colors: colors.clone(),
                positions: positions.clone(),
            }),
            Shading::Radial {
                coords: [start_x, start_y, start_r, end_x, end_y, end_r],
                positions,
                colors,
                ..
            } => {
                // Apply transform adjustments if a matrix is provided.
                let transform = if let Some(mut mat) = *matrix {
                    // FIXME: Converting matrix to the device userspace. The rendering backend expects an
                    // origin at the top-left, with the Y-axis pointing downwards, so we apply canvas height - ty.
                    mat.ty = self.canvas.height() - mat.ty;
                    Some(mat)
                } else {
                    None
                };

                Some(Shader::RadialGradient {
                    start_x: *start_x,
                    start_y: *start_y,
                    start_r: *start_r,
                    end_x: *end_x,
                    end_y: *end_y,
                    end_r: *end_r,
                    transform,
                    colors: colors.clone(),
                    positions: positions.clone(),
                })
            }
            Shading::FunctionBased { .. } => {
                println!("FunctionBased shading not implemented");
                None
            }
        }
    }

    /// Computes the shader and (optional) pattern image based on the current pattern in state.
    /// Returns a tuple of (shader, pattern_image).
    fn compute_shader_and_pattern_image(
        &mut self,
    ) -> Result<(Option<Shader>, Option<U>), PdfCanvasError> {
        use pdf_page::pattern::Pattern;

        let Some(pattern) = self.current_state()?.pattern else {
            return Ok((None, None));
        };

        match pattern {
            Pattern::Shading {
                shading, matrix, ..
            } => {
                let shader = self.build_shading_shader(shading, matrix);
                Ok((shader, None))
            }
            Pattern::Tiling {
                bbox,
                resources,
                content_stream,
                ..
            } => {
                // Create a new mask surface from the backend, sized to the form's bounding box.
                let mut mask = self
                    .canvas
                    .create_mask(bbox[2] - bbox[0], bbox[3] - bbox[1]);

                // Render the tiling content into a temporary canvas.
                let mut other = PdfCanvas::new(mask.as_mut(), self.page, Some(bbox));
                other.render_content_stream(&content_stream.operations, None, Some(resources))?;
                let image = other.canvas.image_snapshot();
                Ok((None, Some(image)))
            }
        }
    }

    pub(crate) fn paint_taken_path(
        &mut self,
        mode: PaintMode,
        fill_type: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        if let Some(mut path) = self.current_path.take() {
            path.transform(&self.current_state()?.transform);
            let (shader, pattern_image) = self.compute_shader_and_pattern_image()?;

            if mode == PaintMode::Fill {
                self.canvas.fill_path(
                    &path,
                    fill_type,
                    self.current_state()?.fill_color,
                    &shader,
                    pattern_image,
                );
            } else {
                self.canvas.stroke_path(
                    &path,
                    self.current_state()?.stroke_color,
                    self.current_state()?.line_width,
                    &shader,
                    pattern_image,
                );
            }
            Ok(())
        } else {
            Err(PdfCanvasError::NoActivePath)
        }
    }

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

    pub(crate) fn render_content_stream(
        &mut self,
        operations: &[PdfOperatorVariant],
        mat: Option<Transform>,
        resources: Option<&'a Resources>,
    ) -> Result<(), PdfCanvasError> {
        self.save()?;

        if let Some(mat) = mat {
            self.set_matrix(mat)?;
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
