use pdf_content_stream::graphics_state_operators::{LineCap, LineJoin};
use pdf_font::font::Font;
use pdf_graphics::{color::Color, transform::Transform};
use pdf_page::{form::FormXObject, page::PdfPage, pattern::Pattern, resources::Resources};

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

impl<'a> Default for TextState<'a> {
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

impl<'a> Default for CanvasState<'a> {
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

pub struct PdfCanvas<'a, T> {
    pub(crate) current_path: Option<PdfPath>,
    pub(crate) canvas: &'a mut dyn CanvasBackend<MaskType = T>,
    pub(crate) mask: Option<Box<T>>,
    pub(crate) page: &'a PdfPage,
    // Stores the graphics states, including text state.
    pub(crate) canvas_stack: Vec<CanvasState<'a>>,
}

impl<T: CanvasBackend> Canvas for PdfCanvas<'_, T> {
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
        let fill_color = self.current_state()?.fill_color;
        self.canvas.fill_path(path, fill_type, fill_color, &None);
        Ok(())
    }
}

impl<'a, T: CanvasBackend> PdfCanvas<'a, T>
where
    T: 'a,
{
    pub fn new(
        backend: &'a mut dyn CanvasBackend<MaskType = T>,
        page: &'a PdfPage,
        bb: Option<&[f32; 4]>,
    ) -> Self {
        let media_box = &page.media_box;

        let pdf_media_width = if let Some(bb) = bb {
            bb[2] - bb[0]
        } else {
            media_box.as_ref().unwrap().width() as f32
        };
        let pdf_media_height = if let Some(bb) = bb {
            bb[3] - bb[1]
        } else {
            media_box.as_ref().unwrap().height() as f32
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

    pub(crate) fn paint_taken_path(
        &mut self,
        mode: PaintMode,
        fill_type: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        if let Some(mut path) = self.current_path.take() {
            path.transform(&self.current_state()?.transform);
            let shader = if let Some(pattern) = self.current_state()?.pattern {
                use pdf_page::pattern::Pattern;
                use pdf_page::shading::Shading;

                if let Pattern::Shading {
                    shading, matrix, ..
                } = pattern
                {
                    println!("pattern");
                    match shading {
                        Shading::Axial {
                            coords: [x0, y0, x1, y1],
                            positions,
                            colors,
                            ..
                        } => {
                            // Construct a LinearShader (or Shader::LinearGradient) using coords and function
                            Some(Shader::LinearGradient {
                                x0: *x0,
                                y0: *y0,
                                x1: *x1,
                                y1: *y1,
                                colors: colors.clone(),
                                positions: positions.clone(),
                            })
                        }
                        Shading::Radial {
                            coords: [start_x, start_y, start_r, end_x, end_y, end_r],
                            positions,
                            colors,
                            ..
                        } => {
                            let transform = if let Some(mut mat) = matrix.clone() {
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
                        Shading::FunctionBased {
                            color_space,
                            background,
                            bbox,
                            anti_alias,
                            domain,
                            functions,
                        } => {
                            println!("FunctionBased");
                            todo!()
                        }
                    }
                } else {
                    println!("No shading");
                    None
                }
            } else {
                None
            };

            if mode == PaintMode::Fill {
                self.canvas
                    .fill_path(&path, fill_type, self.current_state()?.fill_color, &shader);
            } else {
                self.canvas.stroke_path(
                    &path,
                    self.current_state()?.stroke_color,
                    self.current_state()?.line_width,
                    &shader,
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

    pub(crate) fn render_form_xobject(
        &mut self,
        form: &'a FormXObject,
    ) -> Result<(), PdfCanvasError> {
        let form_procs = &form.content_stream.operations;
        self.save()?;

        if let Some(mat) = &form.matrix {
            self.set_matrix(mat.clone())?;
        }

        if let Some(resources) = &form.resources {
            self.current_state_mut()?.resources = Some(resources);
        }

        for op in form_procs {
            op.call(self)?;
        }

        self.restore()
    }
}
