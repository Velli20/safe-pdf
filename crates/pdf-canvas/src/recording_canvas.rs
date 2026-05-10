use std::sync::Arc;

use crate::{
    canvas_backend::{CanvasBackend, Image as BackendImage, Shader},
    error::PdfCanvasError,
};
use pdf_graphics::{
    BlendMode, MaskMode, PathFillType, color::Color, pdf_path::PdfPath, rect::Rect,
    transform::Transform,
};

/// Enum representing each drawing command that can be recorded.
#[derive(Clone)]
enum RecordingCommand {
    FillPath {
        path: PdfPath,
        fill_type: PathFillType,
        color: Color,
        shader: Option<Shader<'static>>,
        blend_mode: Option<BlendMode>,
    },
    StrokePath {
        path: PdfPath,
        color: Color,
        line_width: f32,
        shader: Option<Shader<'static>>,
        blend_mode: Option<BlendMode>,
    },
    SetClipRegion {
        path: PdfPath,
        mode: PathFillType,
    },
    Save,
    Restore,
    DrawImage {
        image: BackendImage<'static>,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    },
    DrawInlineImage {
        image: BackendImage<'static>,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    },
    BeginMaskLayer {
        mask: Arc<RecordingCanvas>,
        transform: Transform,
        mask_mode: MaskMode,
    },
    EndMaskLayer {
        mask: Arc<RecordingCanvas>,
        transform: Transform,
        mask_mode: MaskMode,
    },
}

/// An in-memory, backend-agnostic canvas that records drawing commands.
///
/// `RecordingCanvas` implements `CanvasBackend` trait but does not render. Instead,
/// each drawing operation is captured as a command and stored in
/// sequence for later inspection or replay.
#[derive(Clone)]
pub struct RecordingCanvas {
    /// Logical canvas width used for layout and coordinate space.
    pub width: f32,
    /// Logical canvas height used for layout and coordinate space.
    pub height: f32,
    /// Ordered list of recorded drawing commands.
    commands: Vec<RecordingCommand>,
}

impl<'a> Shader<'a> {
    /// Converts this shader into a `Shader<'static>` by performing a deep clone
    /// of all borrowed data into owned storage. This is useful for storing
    /// shaders in recording commands that must outlive the original borrow.
    fn to_static(&self) -> Shader<'static> {
        match self {
            Shader::Shading(shading) => Shader::Shading(shading.to_static()),
            Shader::TilingPatternImage {
                image,
                transform,
                x_step,
                y_step,
            } => Shader::TilingPatternImage {
                image: Arc::clone(image),
                transform: *transform,
                x_step: *x_step,
                y_step: *y_step,
            },
        }
    }
}

impl RecordingCanvas {
    /// Creates a new recording canvas with the given logical dimensions.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            commands: Vec::new(),
        }
    }

    /// Replays all recorded drawing commands onto the provided backend.
    ///
    /// This method iterates over the internally stored sequence of drawing
    /// operations (paths, images, clip regions, and mask layers) and forwards
    /// them to the given `CanvasBackend` in the original order. Use this to
    /// render a previously captured recording to any concrete backend
    /// implementation (e.g., Skia, FemtoVG, or another `RecordingCanvas`).
    ///
    /// # Parameters
    ///
    /// - `backend`: The target canvas backend to which the recorded commands will be replayed.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if all commands were successfully replayed.
    /// - An error of type `PdfCanvasError` if any command fails during replay.
    pub fn replay<B: CanvasBackend>(&self, backend: &mut B) -> Result<(), PdfCanvasError> {
        use RecordingCommand::*;
        for cmd in &self.commands {
            match cmd {
                FillPath {
                    path,
                    fill_type,
                    color,
                    shader,
                    blend_mode,
                } => {
                    backend.fill_path(path, *fill_type, *color, shader, *blend_mode)?;
                }
                StrokePath {
                    path,
                    color,
                    line_width,
                    shader,
                    blend_mode,
                } => {
                    backend.stroke_path(path, *color, *line_width, shader, *blend_mode)?;
                }
                SetClipRegion { path, mode } => backend.set_clip_region(path, *mode)?,
                Save => backend.save()?,
                Restore => backend.restore()?,
                DrawImage {
                    image,
                    blend_mode,
                    dest_rect,
                    image_rotation,
                } => {
                    backend.draw_image_rect(image, *blend_mode, *dest_rect, *image_rotation)?;
                }
                DrawInlineImage {
                    image,
                    blend_mode,
                    dest_rect,
                    image_rotation,
                } => {
                    backend.draw_inline_image(image, *blend_mode, *dest_rect, *image_rotation)?;
                }
                BeginMaskLayer {
                    transform,
                    mask_mode,
                    mask,
                } => {
                    backend.begin_mask_layer(mask, transform, *mask_mode)?;
                }
                EndMaskLayer {
                    mask,
                    transform,
                    mask_mode,
                } => {
                    backend.end_mask_layer(mask, transform, *mask_mode)?;
                }
            }
        }
        Ok(())
    }
}

impl CanvasBackend for RecordingCanvas {
    fn fill_path(
        &mut self,
        path: &PdfPath,
        fill_type: PathFillType,
        color: Color,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::FillPath {
            path: path.clone(),
            fill_type,
            color,
            shader: shader.as_ref().map(|s| s.to_static()),
            blend_mode,
        });
        Ok(())
    }

    fn stroke_path(
        &mut self,
        path: &PdfPath,
        color: Color,
        line_width: f32,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::StrokePath {
            path: path.clone(),
            color,
            line_width,
            shader: shader.as_ref().map(|s| s.to_static()),
            blend_mode,
        });
        Ok(())
    }

    fn set_clip_region(
        &mut self,
        path: &PdfPath,
        mode: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::SetClipRegion {
            path: path.clone(),
            mode,
        });
        Ok(())
    }

    fn width(&self) -> f32 {
        self.width
    }

    fn height(&self) -> f32 {
        self.height
    }

    fn save(&mut self) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::Save);
        Ok(())
    }

    fn restore(&mut self) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::Restore);
        Ok(())
    }

    fn draw_image_rect(
        &mut self,
        image: &BackendImage<'_>,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::DrawImage {
            image: BackendImage {
                data: image.data.to_shared(),
                width: image.width,
                height: image.height,
                pixel_format: image.pixel_format,
            },
            blend_mode,
            dest_rect,
            image_rotation,
        });
        Ok(())
    }

    fn draw_inline_image(
        &mut self,
        image: &BackendImage<'_>,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::DrawInlineImage {
            image: BackendImage {
                data: image.data.to_shared(),
                width: image.width,
                height: image.height,
                pixel_format: image.pixel_format,
            },
            blend_mode,
            dest_rect,
            image_rotation,
        });
        Ok(())
    }

    fn begin_mask_layer(
        &mut self,
        mask: &Arc<RecordingCanvas>,
        transform: &Transform,
        mask_mode: MaskMode,
    ) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::BeginMaskLayer {
            transform: *transform,
            mask_mode,
            mask: Arc::clone(mask),
        });
        Ok(())
    }

    fn end_mask_layer(
        &mut self,
        mask: &Arc<RecordingCanvas>,
        transform: &Transform,
        mask_mode: MaskMode,
    ) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::EndMaskLayer {
            transform: *transform,
            mask_mode,
            mask: Arc::clone(mask),
        });
        Ok(())
    }
}
