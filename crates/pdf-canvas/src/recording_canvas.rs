//! Flat drawing commands with validated mask bodies and operation-local replay cleanup.

use crate::{
    CanvasPath,
    canvas_backend::{CanvasBackend, Shader},
    error::PdfCanvasError,
    mask_layer::MaskLayer,
    stroke_style::StrokeStyle,
};
use pdf_graphics::{BlendMode, Image as BackendImage, PathFillType, color::Color, rect::Rect};

/// Enum representing each drawing command that can be recorded.
#[derive(Clone)]
enum RecordingCommand {
    /// Fill device-space path geometry.
    FillPath {
        /// Geometry to paint or clip, retaining shared sources and deferred mappings.
        path: CanvasPath<'static>,
        /// Rule determining the path interior.
        fill_type: PathFillType,
        /// Solid paint color, including opacity.
        color: Color,
        /// Optional shading or tiling paint.
        shader: Option<Shader>,
        /// Optional compositing operation.
        blend_mode: Option<BlendMode>,
    },
    /// Stroke device-space path geometry.
    StrokePath {
        /// Geometry to paint or clip, retaining shared sources and deferred mappings.
        path: CanvasPath<'static>,
        /// Solid paint color, including opacity.
        color: Color,
        /// Stroke width in device units.
        line_width: f32,
        /// Caps, joins, and dash metadata.
        stroke_style: StrokeStyle,
        /// Optional shading or tiling paint.
        shader: Option<Shader>,
        /// Optional compositing operation.
        blend_mode: Option<BlendMode>,
    },
    /// Intersect the current clipping region.
    SetClipRegion {
        /// Geometry to paint or clip, retaining shared sources and deferred mappings.
        path: CanvasPath<'static>,
        /// Rule determining the clip interior.
        mode: PathFillType,
    },
    /// Save drawing state within the current lexical body.
    Save,
    /// Restore a save belonging to the current lexical body.
    Restore,
    /// Draw a decoded image object.
    DrawImage {
        /// Shared decoded pixel buffer and dimensions.
        image: BackendImage,
        /// Optional compositing operation.
        blend_mode: Option<BlendMode>,
        /// Image destination in device coordinates.
        dest_rect: Rect,
        /// Optional image rotation in degrees.
        image_rotation: Option<f32>,
    },
    /// Draw decoded inline image pixels.
    DrawInlineImage {
        /// Shared decoded pixel buffer and dimensions.
        image: BackendImage,
        /// Optional compositing operation.
        blend_mode: Option<BlendMode>,
        /// Image destination in device coordinates.
        dest_rect: Rect,
        /// Optional image rotation in degrees.
        image_rotation: Option<f32>,
    },
    /// Scoped content occupies the following `len` commands in this same buffer.
    Mask {
        /// Coverage descriptor applied to this body.
        mask: MaskLayer,
        /// Number of following commands belonging to this body, including nested headers.
        len: usize,
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
    /// Deepest chain of mask and tiling recordings replayed beneath this one.
    nesting: usize,
}

/// Maximum depth of recordings nested through masks and tiling patterns.
///
/// Backends replay nested recordings recursively, so this bounds their stack use
/// and temporary surface count independently of the target.
pub const MAX_NESTING: usize = 64;

impl RecordingCanvas {
    /// Creates a new recording canvas with the given logical dimensions.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            commands: Vec::new(),
            nesting: 0,
        }
    }

    /// Returns the depth of recordings nested beneath this one; zero when flat.
    pub fn nesting(&self) -> usize {
        self.nesting
    }

    /// Raises the nesting depth for a recorded child recording.
    fn nest(&mut self, child: &RecordingCanvas) {
        self.nesting = self.nesting.max(child.nesting.saturating_add(1));
    }

    /// Validates save balance and mask-body boundaries before any destination is touched.
    pub fn validate(&self) -> Result<(), RecordingError> {
        validate_commands(&self.commands)
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
        self.validate()?;
        replay_commands(&self.commands, backend)
    }
}

/// Invalid command nesting or an incomplete mask body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RecordingError {
    /// A save is unmatched or a restore crosses the current body boundary.
    #[error("unbalanced save/restore in recording")]
    UnbalancedSave,
    /// A mask body extends outside its enclosing command slice.
    #[error("invalid mask body in recording")]
    InvalidMaskBody,
}

/// Checks each lexical body independently, preventing restores of caller-owned scopes.
fn validate_commands(mut commands: &[RecordingCommand]) -> Result<(), RecordingError> {
    let mut saves = 0_usize;
    while let Some((command, rest)) = commands.split_first() {
        commands = rest;
        match command {
            RecordingCommand::Save => {
                saves = saves.checked_add(1).ok_or(RecordingError::UnbalancedSave)?
            }
            RecordingCommand::Restore => {
                saves = saves.checked_sub(1).ok_or(RecordingError::UnbalancedSave)?
            }
            RecordingCommand::Mask { len, .. } => {
                let (body, rest) = commands
                    .split_at_checked(*len)
                    .ok_or(RecordingError::InvalidMaskBody)?;
                validate_commands(body)?;
                commands = rest;
            }
            _ => {}
        }
    }
    if saves == 0 {
        Ok(())
    } else {
        Err(RecordingError::UnbalancedSave)
    }
}

/// Replays an already validated body and explicitly restores only its own outstanding saves.
fn replay_commands<B: CanvasBackend>(
    mut commands: &[RecordingCommand],
    backend: &mut B,
) -> Result<(), PdfCanvasError> {
    let mut saves = 0_usize;
    let mut result = (|| {
        use RecordingCommand::*;
        while let Some((cmd, rest)) = commands.split_first() {
            commands = rest;
            match cmd {
                FillPath {
                    path,
                    fill_type,
                    color,
                    shader,
                    blend_mode,
                } => {
                    backend.fill_path(
                        path,
                        *fill_type,
                        *color,
                        shader.as_ref(),
                        blend_mode.clone(),
                    )?;
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
                        shader.as_ref(),
                        blend_mode.clone(),
                    )?;
                }
                SetClipRegion { path, mode } => backend.set_clip_region(path, *mode)?,
                Save => {
                    backend.save()?;
                    saves = saves.checked_add(1).ok_or(RecordingError::UnbalancedSave)?;
                }
                Restore => {
                    saves = saves.checked_sub(1).ok_or(RecordingError::UnbalancedSave)?;
                    backend.restore()?;
                }
                DrawImage {
                    image,
                    blend_mode,
                    dest_rect,
                    image_rotation,
                } => {
                    backend.draw_image_rect(
                        image,
                        blend_mode.clone(),
                        *dest_rect,
                        *image_rotation,
                    )?;
                }
                DrawInlineImage {
                    image,
                    blend_mode,
                    dest_rect,
                    image_rotation,
                } => {
                    backend.draw_inline_image(
                        image,
                        blend_mode.clone(),
                        *dest_rect,
                        *image_rotation,
                    )?;
                }
                Mask { mask, len } => {
                    let (body, rest) = commands
                        .split_at_checked(*len)
                        .ok_or(RecordingError::InvalidMaskBody)?;
                    backend.with_mask_layer(mask, |backend| replay_commands(body, backend))?;
                    commands = rest;
                }
            }
        }
        Ok(())
    })();
    for _ in 0..saves {
        let cleanup = backend.restore();
        result = match (result, cleanup) {
            (Ok(()), restored) => restored,
            (Err(primary), Ok(())) => Err(primary),
            (Err(primary), Err(cleanup)) => Err(PdfCanvasError::Cleanup {
                primary: Box::new(primary),
                cleanup: Box::new(cleanup),
            }),
        };
    }
    result
}

impl CanvasBackend for RecordingCanvas {
    /// Fills the supplied device-space path using the requested paint and fill rule.
    fn fill_path(
        &mut self,
        path: &CanvasPath<'_>,
        fill_type: PathFillType,
        color: Color,
        shader: Option<&Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        if let Some(Shader::TilingPatternImage(pattern)) = shader {
            self.nest(pattern.recording());
        }
        self.commands.push(RecordingCommand::FillPath {
            path: path.clone().into_owned(),
            fill_type,
            color,
            shader: shader.cloned(),
            blend_mode,
        });
        Ok(())
    }

    /// Strokes device-space geometry using the supplied width and stroke style.
    fn stroke_path(
        &mut self,
        path: &CanvasPath<'_>,
        color: Color,
        line_width: f32,
        stroke_style: &StrokeStyle,
        shader: Option<&Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        if let Some(Shader::TilingPatternImage(pattern)) = shader {
            self.nest(pattern.recording());
        }
        self.commands.push(RecordingCommand::StrokePath {
            path: path.clone().into_owned(),
            color,
            line_width,
            stroke_style: stroke_style.clone(),
            shader: shader.cloned(),
            blend_mode,
        });
        Ok(())
    }

    /// Intersects subsequent painting with the supplied device-space clipping path.
    fn set_clip_region(
        &mut self,
        path: &CanvasPath<'_>,
        mode: PathFillType,
    ) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::SetClipRegion {
            path: path.clone().into_owned(),
            mode,
        });
        Ok(())
    }

    /// Returns the logical canvas width used by PDF painting.
    fn width(&self) -> f32 {
        self.width
    }

    /// Returns the logical canvas height used by PDF painting.
    fn height(&self) -> f32 {
        self.height
    }

    /// Saves the current clipping and drawing state for a matching restore.
    fn save(&mut self) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::Save);
        Ok(())
    }

    /// Restores the most recently saved drawing state.
    fn restore(&mut self) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::Restore);
        Ok(())
    }

    /// Draws decoded image pixels into the supplied destination rectangle.
    fn draw_image_rect(
        &mut self,
        image: &BackendImage,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::DrawImage {
            image: image.clone(),
            blend_mode,
            dest_rect,
            image_rotation,
        });
        Ok(())
    }

    /// Draws an inline image using the same placement contract as image objects.
    fn draw_inline_image(
        &mut self,
        image: &BackendImage,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError> {
        self.commands.push(RecordingCommand::DrawInlineImage {
            image: image.clone(),
            blend_mode,
            dest_rect,
            image_rotation,
        });
        Ok(())
    }

    /// Appends a mask body in-place and removes it entirely if painting or validation fails.
    fn with_mask_layer<F>(&mut self, mask: &MaskLayer, paint: F) -> Result<(), PdfCanvasError>
    where
        F: FnOnce(&mut Self) -> Result<(), PdfCanvasError>,
    {
        if mask.is_passthrough() {
            return paint(self);
        }
        self.commands.try_reserve(1)?;
        let start = self.commands.len();
        self.commands.push(RecordingCommand::Mask {
            mask: mask.clone(),
            len: usize::MAX,
        });
        let body_start = self.commands.len();
        let result = paint(self).and_then(|()| {
            let body = self
                .commands
                .get(body_start..)
                .ok_or(RecordingError::InvalidMaskBody)?;
            validate_commands(body)?;
            let len = body.len();
            let Some(RecordingCommand::Mask {
                len: recorded_len, ..
            }) = self.commands.get_mut(start)
            else {
                return Err(RecordingError::InvalidMaskBody.into());
            };
            *recorded_len = len;
            Ok(())
        });
        if result.is_err() {
            self.commands.truncate(start);
        } else {
            self.nest(mask.recording());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use pdf_graphics::{MaskMode, PixelFormat};
    use pdf_graphics::{pdf_path::PdfPath, transform::Transform};
    use pdf_shading::paint::ShadingPaint;
    use std::sync::Arc;

    use super::*;

    /// Returns a balanced descriptor for recording-only lifecycle tests.
    fn test_mask() -> MaskLayer {
        MaskLayer::new(
            Arc::new(RecordingCanvas::new(8.0, 8.0)),
            Transform::identity(),
            MaskMode::Alpha,
            None,
        )
        .unwrap()
    }
    /// Nested masks occupy the same command buffer and failed callbacks remove their complete body.
    #[test]
    fn nested_masks_record_inline_and_failure_truncates_body() {
        let mut recording = RecordingCanvas::new(8.0, 8.0);
        recording.save().unwrap();
        let result = recording.with_mask_layer(&test_mask(), |recording| {
            recording.with_mask_layer(&test_mask(), |recording| recording.save())?;
            Ok(())
        });
        assert!(result.is_err());
        assert!(matches!(
            recording.commands.as_slice(),
            [RecordingCommand::Save]
        ));
        recording.restore().unwrap();
        recording
            .with_mask_layer(&test_mask(), |recording| {
                recording.with_mask_layer(&test_mask(), |recording| {
                    recording.save()?;
                    recording.restore()
                })
            })
            .unwrap();
        recording.validate().unwrap();
        assert!(matches!(
            recording.commands.as_slice(),
            [
                RecordingCommand::Save,
                RecordingCommand::Restore,
                RecordingCommand::Mask { len: 3, .. },
                RecordingCommand::Mask { len: 2, .. },
                RecordingCommand::Save,
                RecordingCommand::Restore
            ]
        ));
        let mut replayed = RecordingCanvas::new(8.0, 8.0);
        recording.replay(&mut replayed).unwrap();
        replayed.validate().unwrap();
    }
    /// Malformed ranges and cross-body restores are rejected before touching the destination.
    #[test]
    fn malformed_recordings_do_not_touch_destination() {
        for commands in [
            vec![RecordingCommand::Mask {
                mask: test_mask(),
                len: 10,
            }],
            vec![
                RecordingCommand::Save,
                RecordingCommand::Mask {
                    mask: test_mask(),
                    len: 1,
                },
                RecordingCommand::Restore,
            ],
            vec![RecordingCommand::Restore],
        ] {
            let recording = RecordingCanvas {
                width: 8.0,
                height: 8.0,
                commands,
                nesting: 0,
            };
            let mut destination = RecordingCanvas::new(8.0, 8.0);
            assert!(recording.replay(&mut destination).is_err());
            assert!(destination.commands.is_empty());
        }
    }
    /// Returned painting errors retain prior commands and never publish a partial mask header.
    #[test]
    fn callback_error_preserves_earlier_commands() {
        let mut recording = RecordingCanvas::new(8.0, 8.0);
        recording.save().unwrap();
        let result = recording.with_mask_layer(&test_mask(), |recording| {
            recording.save()?;
            Err(PdfCanvasError::CurrentPointRequired)
        });
        assert!(matches!(result, Err(PdfCanvasError::CurrentPointRequired)));
        assert!(matches!(
            recording.commands.as_slice(),
            [RecordingCommand::Save]
        ));
        recording.restore().unwrap();
        recording.validate().unwrap();
    }

    #[test]
    /// Verifies that failed masked paint discards recorded body.
    fn failed_masked_paint_discards_recorded_body() {
        let page = pdf_document::page::PdfPage::default();
        let mut recording = RecordingCanvas::new(128.0, 128.0);
        let mut canvas = crate::pdf_canvas::PdfCanvas::new(
            &mut recording,
            &page,
            None,
            pdf_text_engine::bundled_font_system(),
        )
        .unwrap();
        canvas.current_state_mut().unwrap().soft_mask = Some(
            MaskLayer::new(
                Arc::new(RecordingCanvas::new(128.0, 128.0)),
                Transform::identity(),
                MaskMode::Alpha,
                None,
            )
            .unwrap(),
        );
        let result = canvas.with_soft_mask(|_| Err(PdfCanvasError::CurrentPointRequired));
        assert!(matches!(result, Err(PdfCanvasError::CurrentPointRequired)));
        assert!(recording.commands.is_empty());
    }

    #[test]
    /// Verifies that recorded image shares pixel data.
    fn recorded_image_shares_pixel_data() {
        let data = bytes::Bytes::from_static(&[1, 2, 3, 4]);
        let image = BackendImage {
            data: data.clone(),
            width: 1,
            height: 1,
            pixel_format: PixelFormat::RGBA8888,
        };
        let mut canvas = RecordingCanvas::new(1.0, 1.0);

        canvas
            .draw_image_rect(&image, None, Rect::UNIT_RECT, None)
            .expect("image should be recorded");

        assert!(canvas.commands.iter().any(|command| {
            matches!(
                command,
                RecordingCommand::DrawImage { image, .. }
                    if image.data.as_ptr() == data.as_ptr()
            )
        }));
    }

    #[test]
    /// Verifies that recorded gradient and canvas clone share color stops.
    fn recorded_gradient_and_canvas_clone_share_color_stops() {
        let colors: Arc<[Color]> = [
            Color::from_rgb(0.0, 0.0, 0.0),
            Color::from_rgb(1.0, 1.0, 1.0),
        ]
        .into();
        let positions: Arc<[f32]> = [0.0, 1.0].into();
        let shader = Some(Shader::Shading(ShadingPaint::LinearGradient {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
            transform: Transform::identity(),
            colors: Arc::clone(&colors),
            positions: Arc::clone(&positions),
        }));
        let mut canvas = RecordingCanvas::new(1.0, 1.0);

        canvas
            .fill_path(
                &CanvasPath::device(&PdfPath::default()),
                PathFillType::Winding,
                Color::from_rgb(0.0, 0.0, 0.0),
                shader.as_ref(),
                None,
            )
            .expect("gradient should be recorded");
        let cloned_canvas = canvas.clone();

        for recording in [&canvas, &cloned_canvas] {
            assert!(recording.commands.iter().any(|command| {
                matches!(
                    command,
                    RecordingCommand::FillPath {
                        shader:
                            Some(Shader::Shading(ShadingPaint::LinearGradient {
                                colors: recorded_colors,
                                positions: recorded_positions,
                                ..
                            })),
                        ..
                    } if Arc::ptr_eq(recorded_colors, &colors)
                        && Arc::ptr_eq(recorded_positions, &positions)
                )
            }));
        }
    }

    #[test]
    /// Verifies that transformed path recording shares source geometry.
    fn transformed_path_recording_shares_source_geometry() {
        let mut source = PdfPath::default();
        source.move_to(1.0, 2.0);
        source.line_to(3.0, 4.0);
        let source = Arc::new(source);
        let mut canvas = RecordingCanvas::new(10.0, 10.0);

        canvas
            .fill_path(
                &CanvasPath::shared(Arc::clone(&source), Transform::from_translate(5.0, 6.0))
                    .expect("finite instance transform"),
                PathFillType::Winding,
                Color::from_rgb(0.0, 0.0, 0.0),
                None,
                None,
            )
            .expect("transformed path should be recorded");

        assert!(matches!(
            canvas.commands.first(),
            Some(RecordingCommand::FillPath { path, .. })
                if std::ptr::eq(path.source(), source.as_ref())
        ));
    }
}
