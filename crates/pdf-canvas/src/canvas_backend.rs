use std::sync::Arc;

use pdf_graphics::{
    BlendMode, Image, MaskMode, PathFillType, color::Color, pdf_path::PdfPath, rect::Rect,
    transform::Transform,
};
use pdf_shading::paint::ShadingPaint;

use crate::{error::PdfCanvasError, recording_canvas::RecordingCanvas, stroke_style::StrokeStyle};

/// Represents a shader used for advanced fill and stroke operations in PDF rendering.
#[derive(Clone)]
pub enum Shader {
    /// A shading-backed shader such as an axial gradient, radial gradient, or mesh raster.
    Shading(ShadingPaint),
    /// Represents a tiling pattern image shader for filling or stroking paths with a repeated image.
    ///
    /// Used to define how an image is tiled across a region, with optional transformation and spacing.
    TilingPatternImage {
        /// A recording canvas containing the tiling pattern content.
        image: Arc<RecordingCanvas>,
        /// The transformation to apply to the pattern tile.
        transform: Option<Transform>,
        /// The horizontal spacing between tiles.
        x_step: f32,
        /// The vertical spacing between tiles.
        y_step: f32,
    },
}

/// A low-level drawing backend for rendering PDF graphics.
///
/// This trait defines the fundamental drawing operations that a `PdfCanvas` uses
/// to render content. Implementors of this trait act as the target surface,
/// such as a raster image buffer, a window, or an SVG file.
pub trait CanvasBackend {
    /// Fills the given path with the specified color and fill rule.
    ///
    /// # Parameters
    ///
    /// - `path`: The path to fill. The coordinates are in the backend's device space.
    /// - `fill_type`: The rule (winding or even-odd) to determine what is "inside" the path.
    /// - `color`: The color to use for filling the path.
    /// - `shader`: An optional shader to use for filling the path.
    /// - `blend_mode`: An optional blend mode to use when filling the path.
    fn fill_path(
        &mut self,
        path: &PdfPath,
        fill_type: PathFillType,
        color: Color,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError>;

    /// Strokes the given path with the specified color and line width.
    ///
    /// # Parameters
    ///
    /// - `path`: The path to stroke. The coordinates are in the backend's device space.
    /// - `color`: The color of the stroke.
    /// - `line_width`: The width of the stroke in device units.
    /// - `stroke_style`: Stroke metadata such as dash pattern.
    /// - `shader`: An optional shader to use for the stroke.
    /// - `blend_mode`: An optional blend mode to use when stroking the path.
    fn stroke_path(
        &mut self,
        path: &PdfPath,
        color: Color,
        line_width: f32,
        stroke_style: &StrokeStyle,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError>;

    /// Fills a shared path after applying `transform` to its geometry.
    ///
    /// Backends that can retain shared geometry or apply native transforms should override this
    /// method. The default implementation preserves compatibility by materializing a transformed
    /// device-space path and forwarding to [`Self::fill_path`].
    fn fill_transformed_path(
        &mut self,
        path: &Arc<PdfPath>,
        transform: &Transform,
        fill_type: PathFillType,
        color: Color,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        let mut transformed = path.as_ref().clone();
        transformed.transform(transform);
        self.fill_path(&transformed, fill_type, color, shader, blend_mode)
    }

    /// Strokes a shared path after applying `transform` to its geometry.
    ///
    /// The default implementation materializes a transformed device-space path and forwards to
    /// [`Self::stroke_path`].
    #[allow(clippy::too_many_arguments)]
    fn stroke_transformed_path(
        &mut self,
        path: &Arc<PdfPath>,
        transform: &Transform,
        color: Color,
        line_width: f32,
        stroke_style: &StrokeStyle,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError> {
        let mut transformed = path.as_ref().clone();
        transformed.transform(transform);
        self.stroke_path(
            &transformed,
            color,
            line_width,
            stroke_style,
            shader,
            blend_mode,
        )
    }

    /// Sets the clipping region by intersecting the current clip path with the given path.
    ///
    /// All subsequent drawing operations will be constrained to this new region.
    ///
    /// # Parameters
    ///
    /// - `path`: The path to use for clipping.
    /// - `mode`: The fill type to determine the clipping region.
    fn set_clip_region(&mut self, path: &PdfPath, mode: PathFillType)
    -> Result<(), PdfCanvasError>;

    /// Returns the width of the canvas in device units.
    fn width(&self) -> f32;

    /// Returns the height of the canvas in device units.
    fn height(&self) -> f32;

    /// Saves the current graphics state (transform, clip, etc.).
    fn save(&mut self) -> Result<(), PdfCanvasError>;

    /// Restores the most recently saved graphics state.
    fn restore(&mut self) -> Result<(), PdfCanvasError>;

    /// Draws an image onto the canvas at the current transformation.
    ///
    /// # Parameters
    ///
    /// - `image`: The image to draw.
    /// - `blend_mode`: Optional blend mode to use when compositing the image.
    /// - `dest_rect`: The destination rectangle on the canvas where the image should be drawn.
    /// - `image_rotation`: An optional rotation (in degrees) to apply to the image.
    fn draw_image_rect(
        &mut self,
        image: &Image,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError>;

    /// Draws an inline image onto the canvas.
    ///
    /// The default implementation forwards to [`CanvasBackend::draw_image_rect`].
    fn draw_inline_image(
        &mut self,
        image: &Image,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError> {
        self.draw_image_rect(image, blend_mode, dest_rect, image_rotation)
    }

    /// Begins drawing into the specified mask layer.
    ///
    /// All subsequent drawing operations will affect the mask until `end_mask_layer` is called.
    ///
    /// # Parameters
    ///
    /// - `mask`: The mask layer to begin drawing into.
    fn begin_mask_layer(
        &mut self,
        mask: &Arc<RecordingCanvas>,
        transform: &Transform,
        mask_mode: MaskMode,
    ) -> Result<(), PdfCanvasError>;

    /// Ends drawing into the specified mask layer and applies it to the canvas.
    ///
    /// # Parameters
    ///
    /// - `mask`: The mask layer to end and apply.
    /// - `transform`: The transformation to apply to the mask when compositing.
    fn end_mask_layer(
        &mut self,
        mask: &Arc<RecordingCanvas>,
        transform: &Transform,
        mask_mode: MaskMode,
    ) -> Result<(), PdfCanvasError>;
}
