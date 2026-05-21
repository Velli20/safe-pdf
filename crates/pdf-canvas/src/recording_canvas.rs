use std::sync::Arc;

use crate::{
    canvas_backend::{CanvasBackend, Image as BackendImage, Shader},
    error::PdfCanvasError,
    stroke_style::StrokeStyle,
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
        stroke_style: StrokeStyle,
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::stroke_style::StrokeStyle;
    use pdf_graphics::DashPattern;

    #[derive(Default)]
    struct StrokeStyleCanvas {
        last_stroke_style: Option<StrokeStyle>,
    }

    impl CanvasBackend for StrokeStyleCanvas {
        fn fill_path(
            &mut self,
            _path: &PdfPath,
            _fill_type: PathFillType,
            _color: Color,
            _shader: &Option<Shader>,
            _blend_mode: Option<BlendMode>,
        ) -> Result<(), PdfCanvasError> {
            Ok(())
        }

        fn stroke_path(
            &mut self,
            _path: &PdfPath,
            _color: Color,
            _line_width: f32,
            stroke_style: &StrokeStyle,
            _shader: &Option<Shader>,
            _blend_mode: Option<BlendMode>,
        ) -> Result<(), PdfCanvasError> {
            self.last_stroke_style = Some(stroke_style.clone());
            Ok(())
        }

        fn set_clip_region(
            &mut self,
            _path: &PdfPath,
            _mode: PathFillType,
        ) -> Result<(), PdfCanvasError> {
            Ok(())
        }

        fn width(&self) -> f32 {
            100.0
        }

        fn height(&self) -> f32 {
            100.0
        }

        fn save(&mut self) -> Result<(), PdfCanvasError> {
            Ok(())
        }

        fn restore(&mut self) -> Result<(), PdfCanvasError> {
            Ok(())
        }

        fn draw_image_rect(
            &mut self,
            _image: &BackendImage<'_>,
            _blend_mode: Option<BlendMode>,
            _dest_rect: Rect,
            _image_rotation: Option<f32>,
        ) -> Result<(), PdfCanvasError> {
            Ok(())
        }

        fn begin_mask_layer(
            &mut self,
            _mask: &Arc<RecordingCanvas>,
            _transform: &Transform,
            _mask_mode: MaskMode,
        ) -> Result<(), PdfCanvasError> {
            Ok(())
        }

        fn end_mask_layer(
            &mut self,
            _mask: &Arc<RecordingCanvas>,
            _transform: &Transform,
            _mask_mode: MaskMode,
        ) -> Result<(), PdfCanvasError> {
            Ok(())
        }
    }

    #[test]
    fn replay_preserves_stroke_style() {
        let mut recording = RecordingCanvas::new(100.0, 100.0);
        let mut path = PdfPath::default();
        path.move_to(0.0, 0.0);
        path.line_to(10.0, 0.0);
        let stroke_style = StrokeStyle {
            dash_pattern: Some(DashPattern {
                intervals: vec![4.0, 2.0],
                phase: 1.0,
            }),
        };

        recording
            .stroke_path(
                &path,
                Color::from_rgb(0.0, 0.0, 0.0),
                1.0,
                &stroke_style,
                &None,
                None,
            )
            .expect("stroke should record");

        let mut backend = StrokeStyleCanvas::default();
        recording.replay(&mut backend).expect("replay should work");

        assert_eq!(backend.last_stroke_style, Some(stroke_style));
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
                    stroke_style,
                    shader,
                    blend_mode,
                } => {
                    backend.stroke_path(
                        path,
                        *color,
                        *line_width,
                        stroke_style,
                        shader,
                        *blend_mode,
                    )?;
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
        stroke_style: &StrokeStyle,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::StrokePath {
            path: path.clone(),
            color,
            line_width,
            stroke_style: stroke_style.clone(),
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
