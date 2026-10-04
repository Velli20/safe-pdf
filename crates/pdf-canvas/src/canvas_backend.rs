//! Low-level drawing and lexically scoped masking.
use pdf_graphics::{BlendMode, Image, PathFillType, color::Color, transform::Transform};
use pdf_shading::paint::ShadingPaint;

use crate::{
    CanvasPath, error::PdfCanvasError, mask_layer::MaskLayer, stroke_style::StrokeStyle,
    tiling_shader::TilingShader,
};

/// Represents a shader used for advanced fill and stroke operations in PDF rendering.
#[derive(Clone)]
pub enum Shader {
    /// A shading-backed shader such as an axial gradient, radial gradient, or mesh raster.
    Shading(ShadingPaint),
    /// Represents a tiling pattern image shader for filling or stroking paths with a repeated image.
    ///
    /// Used to define how an image is tiled across a region, with optional transformation and spacing.
    TilingPatternImage(TilingShader),
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
    /// - `path`: The path to fill. Its verbs resolve to logical device coordinates.
    /// - `fill_type`: The rule (winding or even-odd) to determine what is "inside" the path.
    /// - `color`: The color to use for filling the path.
    /// - `shader`: An optional shader to use for filling the path.
    /// - `blend_mode`: An optional blend mode to use when filling the path.
    fn fill_path(
        &mut self,
        path: &CanvasPath<'_>,
        fill_type: PathFillType,
        color: Color,
        shader: Option<&Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError>;

    /// Strokes the given path with the specified color and line width.
    ///
    /// # Parameters
    ///
    /// - `path`: The path to stroke. Its verbs resolve to logical device coordinates.
    /// - `color`: The color of the stroke.
    /// - `line_width`: The width of the stroke in device units.
    /// - `stroke_style`: Stroke metadata such as dash pattern.
    /// - `shader`: An optional shader to use for the stroke.
    /// - `blend_mode`: An optional blend mode to use when stroking the path.
    fn stroke_path(
        &mut self,
        path: &CanvasPath<'_>,
        color: Color,
        line_width: f32,
        stroke_style: &StrokeStyle,
        shader: Option<&Shader>,
        blend_mode: Option<BlendMode>,
    ) -> Result<(), PdfCanvasError>;

    /// Sets the clipping region by intersecting the current clip path with the given path.
    ///
    /// All subsequent drawing operations will be constrained to this new region.
    ///
    /// # Parameters
    ///
    /// - `path`: The path to use for clipping.
    /// - `mode`: The fill type to determine the clipping region.
    fn set_clip_region(
        &mut self,
        path: &CanvasPath<'_>,
        mode: PathFillType,
    ) -> Result<(), PdfCanvasError>;

    /// Returns the width of the canvas in device units.
    fn width(&self) -> f32;

    /// Returns the height of the canvas in device units.
    fn height(&self) -> f32;

    /// Saves the current graphics state (transform, clip, etc.).
    fn save(&mut self) -> Result<(), PdfCanvasError>;

    /// Restores the most recently saved graphics state.
    fn restore(&mut self) -> Result<(), PdfCanvasError>;

    /// Draws an image onto the canvas.
    ///
    /// # Parameters
    ///
    /// - `image`: The image to draw.
    /// - `blend_mode`: Optional blend mode to use when compositing the image.
    /// - `transform`: Maps the unit square to device space, with the image's first pixel
    ///   row along `y = 0` and its first column along `x = 0`.
    fn draw_image(
        &mut self,
        image: &Image,
        blend_mode: Option<BlendMode>,
        transform: Transform,
    ) -> Result<(), PdfCanvasError>;

    /// Draws an inline image onto the canvas.
    ///
    /// The default implementation forwards to [`CanvasBackend::draw_image`].
    fn draw_inline_image(
        &mut self,
        image: &Image,
        blend_mode: Option<BlendMode>,
        transform: Transform,
    ) -> Result<(), PdfCanvasError> {
        self.draw_image(image, blend_mode, transform)
    }

    /// Paints isolated content and applies the supplied mask only after successful painting.
    ///
    /// Preparation failures do not invoke `paint`. Painting failures discard isolated content.
    /// Native restoration runs explicitly before this method returns; it does not use `Drop`.
    /// Unknown mask modes invoke `paint` directly without isolation.
    ///
    /// The callback must balance its own saves/restores and must not restore caller-owned state.
    /// These guarantees apply to returned errors, not unwinding panics.
    fn with_mask_layer<F>(&mut self, mask: &MaskLayer, paint: F) -> Result<(), PdfCanvasError>
    where
        Self: Sized,
        F: FnOnce(&mut Self) -> Result<(), PdfCanvasError>;
}
