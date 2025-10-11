use std::borrow::Cow;

use crate::canvas_backend::{CanvasBackend, Image as BackendImage, Shader};
use pdf_graphics::{
    BlendMode, ImageEncoding, MaskMode, PathFillType, color::Color, pdf_path::PdfPath,
    transform::Transform,
};
use thiserror::Error;

/// Errors that can occur during recording operations.
#[derive(Error, Debug)]
pub enum RecordingCanvasError {
    #[error("Mask layer with index {0} not found")]
    MaskLayerNotFound(usize),
    #[error("Unsupported feature in RecordingCanvas: {0}")]
    UnsupportedFeature(&'static str),
}

/// Owned representation of an image.
#[derive(Clone)]
struct RecordedImage {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub bytes_per_pixel: Option<u32>,
    pub encoding: ImageEncoding,
    pub transform: Transform,
    pub mask: Option<Vec<u8>>,
}

impl From<&BackendImage<'_>> for RecordedImage {
    fn from(img: &BackendImage<'_>) -> Self {
        Self {
            data: img.data.clone().into_owned(),
            width: img.width,
            height: img.height,
            bytes_per_pixel: img.bytes_per_pixel,
            encoding: img.encoding,
            transform: img.transform,
            mask: img.mask.as_ref().map(|m| m.clone().into_owned()),
        }
    }
}

/// Owned representation of a shader.
#[derive(Clone)]
enum RecordedShader {
    LinearGradient {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        colors: Vec<Color>,
        positions: Vec<f32>,
    },
    TilingPatternImage {
        image: Box<RecordingCanvas>,
        transform: Option<Transform>,
        x_step: f32,
        y_step: f32,
    },
    RadialGradient {
        start_x: f32,
        start_y: f32,
        start_r: f32,
        end_x: f32,
        end_y: f32,
        end_r: f32,
        colors: Vec<Color>,
        positions: Vec<f32>,
        transform: Option<Transform>,
    },
}

impl From<&Shader<'_>> for RecordedShader {
    fn from(shader: &Shader<'_>) -> Self {
        match shader {
            Shader::LinearGradient {
                x0,
                y0,
                x1,
                y1,
                colors,
                positions,
            } => Self::LinearGradient {
                x0: *x0,
                y0: *y0,
                x1: *x1,
                y1: *y1,
                colors: (*colors).to_vec(),
                positions: (*positions).to_vec(),
            },
            Shader::TilingPatternImage {
                image,
                transform,
                x_step,
                y_step,
            } => Self::TilingPatternImage {
                image: Box::new((**image).clone()),
                transform: *transform,
                x_step: *x_step,
                y_step: *y_step,
            },
            Shader::RadialGradient {
                start_x,
                start_y,
                start_r,
                end_x,
                end_y,
                end_r,
                colors,
                positions,
                transform,
            } => Self::RadialGradient {
                start_x: *start_x,
                start_y: *start_y,
                start_r: *start_r,
                end_x: *end_x,
                end_y: *end_y,
                end_r: *end_r,
                colors: (*colors).to_vec(),
                positions: (*positions).to_vec(),
                transform: *transform,
            },
        }
    }
}

/// Enum representing each drawing command that can be recorded.
#[derive(Clone)]
enum RecordingCommand {
    FillPath {
        path: PdfPath,
        fill_type: PathFillType,
        color: Color,
        shader: Option<RecordedShader>,
        blend_mode: Option<BlendMode>,
    },
    StrokePath {
        path: PdfPath,
        color: Color,
        line_width: f32,
        shader: Option<RecordedShader>,
        blend_mode: Option<BlendMode>,
    },
    SetClipRegion {
        path: PdfPath,
        mode: PathFillType,
    },
    ResetClip,
    DrawImage {
        image: RecordedImage,
        blend_mode: Option<BlendMode>,
    },
    BeginMaskLayer {
        mask: Box<RecordingCanvas>,
        transform: Transform,
        mask_mode: MaskMode,
    },
    EndMaskLayer {
        mask: Box<RecordingCanvas>,
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
    width: f32,
    /// Logical canvas height used for layout and coordinate space.
    height: f32,
    /// Ordered list of recorded drawing commands.
    commands: Vec<RecordingCommand>,
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
    /// - An error of type `B::ErrorType` if any command fails during replay.
    pub fn replay<B: CanvasBackend>(&self, backend: &mut B) -> Result<(), B::ErrorType> {
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
                    let shader_ref: Option<Shader> = shader.as_ref().map(|s| match s {
                        RecordedShader::LinearGradient {
                            x0,
                            y0,
                            x1,
                            y1,
                            colors,
                            positions,
                        } => Shader::LinearGradient {
                            x0: *x0,
                            y0: *y0,
                            x1: *x1,
                            y1: *y1,
                            colors,
                            positions,
                        },
                        RecordedShader::TilingPatternImage {
                            image,
                            transform,
                            x_step,
                            y_step,
                        } => Shader::TilingPatternImage {
                            image: image.clone(),
                            transform: *transform,
                            x_step: *x_step,
                            y_step: *y_step,
                        },
                        RecordedShader::RadialGradient {
                            start_x,
                            start_y,
                            start_r,
                            end_x,
                            end_y,
                            end_r,
                            colors,
                            positions,
                            transform,
                        } => Shader::RadialGradient {
                            start_x: *start_x,
                            start_y: *start_y,
                            start_r: *start_r,
                            end_x: *end_x,
                            end_y: *end_y,
                            end_r: *end_r,
                            colors,
                            positions,
                            transform: *transform,
                        },
                    });
                    backend.fill_path(path, *fill_type, *color, &shader_ref, *blend_mode)?;
                }
                StrokePath {
                    path,
                    color,
                    line_width,
                    shader,
                    blend_mode,
                } => {
                    let shader_ref: Option<Shader> = shader.as_ref().map(|s| match s {
                        RecordedShader::LinearGradient {
                            x0,
                            y0,
                            x1,
                            y1,
                            colors,
                            positions,
                        } => Shader::LinearGradient {
                            x0: *x0,
                            y0: *y0,
                            x1: *x1,
                            y1: *y1,
                            colors,
                            positions,
                        },
                        RecordedShader::TilingPatternImage {
                            image,
                            transform,
                            x_step,
                            y_step,
                        } => Shader::TilingPatternImage {
                            image: image.clone(),
                            transform: *transform,
                            x_step: *x_step,
                            y_step: *y_step,
                        },
                        RecordedShader::RadialGradient {
                            start_x,
                            start_y,
                            start_r,
                            end_x,
                            end_y,
                            end_r,
                            colors,
                            positions,
                            transform,
                        } => Shader::RadialGradient {
                            start_x: *start_x,
                            start_y: *start_y,
                            start_r: *start_r,
                            end_x: *end_x,
                            end_y: *end_y,
                            end_r: *end_r,
                            colors,
                            positions,
                            transform: *transform,
                        },
                    });
                    backend.stroke_path(path, *color, *line_width, &shader_ref, *blend_mode)?;
                }
                SetClipRegion { path, mode } => backend.set_clip_region(path, *mode)?,
                ResetClip => backend.reset_clip()?,
                DrawImage { image, blend_mode } => {
                    let backend_img = BackendImage {
                        data: Cow::Owned(image.data.clone()),
                        width: image.width,
                        height: image.height,
                        bytes_per_pixel: image.bytes_per_pixel,
                        encoding: image.encoding,
                        transform: image.transform,
                        mask: image.mask.as_ref().map(|m| Cow::Owned(m.clone())),
                    };
                    backend.draw_image(&backend_img, *blend_mode)?;
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
    type ErrorType = RecordingCanvasError;

    fn fill_path(
        &mut self,
        path: &PdfPath,
        fill_type: PathFillType,
        color: Color,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), Self::ErrorType> {
        self.commands.push(RecordingCommand::FillPath {
            path: path.clone(),
            fill_type,
            color,
            shader: shader.as_ref().map(|s| s.into()),
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
    ) -> Result<(), Self::ErrorType> {
        self.commands.push(RecordingCommand::StrokePath {
            path: path.clone(),
            color,
            line_width,
            shader: shader.as_ref().map(|s| s.into()),
            blend_mode,
        });
        Ok(())
    }

    fn set_clip_region(
        &mut self,
        path: &PdfPath,
        mode: PathFillType,
    ) -> Result<(), Self::ErrorType> {
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

    fn reset_clip(&mut self) -> Result<(), Self::ErrorType> {
        self.commands.push(RecordingCommand::ResetClip);
        Ok(())
    }

    fn draw_image(
        &mut self,
        image: &BackendImage<'_>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), Self::ErrorType> {
        self.commands.push(RecordingCommand::DrawImage {
            image: image.into(),
            blend_mode,
        });
        Ok(())
    }

    fn begin_mask_layer(
        &mut self,
        mask: &RecordingCanvas,
        transform: &Transform,
        mask_mode: MaskMode,
    ) -> Result<(), Self::ErrorType> {
        self.commands.push(RecordingCommand::BeginMaskLayer {
            transform: *transform,
            mask_mode,
            mask: Box::new(mask.clone()),
        });
        Ok(())
    }

    fn end_mask_layer(
        &mut self,
        _mask: &RecordingCanvas,
        transform: &Transform,
        mask_mode: MaskMode,
    ) -> Result<(), Self::ErrorType> {
        self.commands.push(RecordingCommand::EndMaskLayer {
            transform: *transform,
            mask_mode,
            mask: Box::new(_mask.clone()),
        });
        Ok(())
    }
}
