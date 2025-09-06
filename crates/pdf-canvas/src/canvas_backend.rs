use pdf_graphics::{
    BlendMode, PathFillType, color::Color, pdf_path::PdfPath, transform::Transform,
};

pub enum Shader<'a> {
    /// Represents a color shader for filling or stroking paths with gradients.
    ///
    /// Used to define how colors are interpolated across a region, such as a linear or radial gradient.
    LinearGradient {
        /// The starting x-coordinate of the gradient line.
        x0: f32,
        /// The starting y-coordinate of the gradient line.
        y0: f32,
        /// The ending x-coordinate of the gradient line.
        x1: f32,
        /// The ending y-coordinate of the gradient line.
        y1: f32,
        /// The array of colors to be used in the gradient.
        colors: &'a [Color],
        /// The positions of each color stop, specified as values between 0.0 and 1.0.
        positions: &'a [f32],
    },
    /// A radial gradient shader, interpolating colors between two circles.
    RadialGradient {
        /// The x-coordinate of the start circle's center.
        start_x: f32,
        /// The y-coordinate of the start circle's center.
        start_y: f32,
        /// The radius of the start circle.
        start_r: f32,
        /// The x-coordinate of the end circle's center.
        end_x: f32,
        /// The y-coordinate of the end circle's center.
        end_y: f32,
        /// The radius of the end circle.
        end_r: f32,
        /// The array of colors to be used in the gradient.
        colors: &'a [Color],
        /// The positions of each color stop, specified as values between 0.0 and 1.0.
        positions: &'a [f32],
        /// An optional transformation to apply to the gradient.
        transform: Option<Transform>,
    },
}

/// A low-level drawing backend for rendering PDF graphics.
///
/// This trait defines the fundamental drawing operations that a `PdfCanvas` uses
/// to render content. Implementors of this trait act as the target surface,
/// such as a raster image buffer, a window, or an SVG file.
pub trait CanvasBackend {
    type MaskType: CanvasBackend<ImageType = Self::ImageType>;
    type ImageType;

    /// Fills the given path with the specified color and fill rule.
    ///
    /// # Parameters
    ///
    /// - `path`: The path to fill. The coordinates are in the backend's device space.
    /// - `fill_type`: The rule (winding or even-odd) to determine what is "inside" the path.
    /// - `color`: The color to use for filling the path.
    fn fill_path(
        &mut self,
        path: &PdfPath,
        fill_type: PathFillType,
        color: Color,
        shader: &Option<Shader>,
        pattern_image: Option<Self::ImageType>,
        blend_mode: Option<BlendMode>,
    );

    /// Strokes the given path with the specified color and line width.
    ///
    /// # Parameters
    ///
    /// - `path`: The path to stroke. The coordinates are in the backend's device space.
    /// - `color`: The color of the stroke.
    /// - `line_width`: The width of the stroke in device units.
    fn stroke_path(
        &mut self,
        path: &PdfPath,
        color: Color,
        line_width: f32,
        shader: &Option<Shader>,
        pattern_image: Option<Self::ImageType>,
        blend_mode: Option<BlendMode>,
    );

    /// Sets the clipping region by intersecting the current clip path with the given path.
    ///
    /// All subsequent drawing operations will be constrained to this new region.
    fn set_clip_region(&mut self, path: &PdfPath, mode: PathFillType);

    /// Returns the width of the canvas in device units.
    fn width(&self) -> f32;

    /// Returns the height of the canvas in device units.
    fn height(&self) -> f32;

    /// Resets the clipping region to the entire canvas area.
    fn reset_clip(&mut self);

    #[allow(clippy::too_many_arguments)]
    fn draw_image(
        &mut self,
        image: &[u8],
        is_jpeg: bool,
        width: f32,
        height: f32,
        bits_per_component: u32,
        transform: &Transform,
        smask: Option<&[u8]>,
    );

    fn create_mask(&mut self, width: f32, height: f32) -> Box<Self::MaskType>;

    fn enable_mask(&mut self, mask: &mut Self::MaskType);

    fn finish_mask(&mut self, mask: &mut Self::MaskType, transform: &Transform);

    fn image_snapshot(&mut self) -> Self::ImageType;
}
