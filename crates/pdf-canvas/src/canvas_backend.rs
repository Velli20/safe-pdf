use std::borrow::Cow;

use pdf_graphics::{
    BlendMode, PathFillType, color::Color, pdf_path::PdfPath, transform::Transform,
};

/// Represents a shader used for advanced fill and stroke operations in PDF rendering.
///
/// A `Shader` defines how colors or patterns are applied to graphical elements, such
/// as gradients or tiling patterns. It is used to enable effects like linear gradients,
/// radial gradients, and image-based patterns when filling or stroking paths.
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
    /// Represents a tiling pattern image shader for filling or stroking paths with a repeated image.
    ///
    /// Used to define how an image is tiled across a region, with optional transformation and spacing.
    TilingPatternImage {
        /// The image to be used as the pattern tile.
        image: Image<'a>,
        /// The transformation to apply to the pattern tile.
        transform: Option<Transform>,
        /// The horizontal spacing between tiles.
        x_step: f32,
        /// The vertical spacing between tiles.
        y_step: f32,
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

/// Represents an image resource for drawing or pattern tiling in the PDF canvas backend.
///
/// The `Image` struct encapsulates raw image data, dimensions, encoding, and optional
/// transformation or masking information.
pub struct Image<'a> {
    /// The raw image data.
    pub data: Cow<'a, [u8]>,
    /// The width of the image in pixels.
    pub width: u32,
    /// The height of the image in pixels.
    pub height: u32,
    /// The bits per pixel (color depth) of the image.
    pub bytes_per_pixel: Option<u32>,
    /// The image encoding (e.g., "jpeg", "png").
    pub encoding: Option<String>,
    /// A transformation matrix to apply to the image.
    pub transform: Transform,
    /// An optional alpha mask to apply to the image.
    pub mask: Option<Cow<'a, [u8]>>,
}

/// A low-level drawing backend for rendering PDF graphics.
///
/// This trait defines the fundamental drawing operations that a `PdfCanvas` uses
/// to render content. Implementors of this trait act as the target surface,
/// such as a raster image buffer, a window, or an SVG file.
pub trait CanvasBackend {
    /// The associated type representing a mask layer backend.
    ///
    /// This type is used for creating and manipulating mask layers, which are offscreen surfaces
    /// that can be drawn into and later composited onto the main canvas. Implementors should provide
    /// a concrete type that also implements `CanvasBackend`, allowing recursive composition of mask layers.
    type MaskType: CanvasBackend;

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
    );

    /// Strokes the given path with the specified color and line width.
    ///
    /// # Parameters
    ///
    /// - `path`: The path to stroke. The coordinates are in the backend's device space.
    /// - `color`: The color of the stroke.
    /// - `line_width`: The width of the stroke in device units.
    /// - `shader`: An optional shader to use for the stroke.
    /// - `blend_mode`: An optional blend mode to use when stroking the path.
    fn stroke_path(
        &mut self,
        path: &PdfPath,
        color: Color,
        line_width: f32,
        shader: &Option<Shader>,
        blend_mode: Option<BlendMode>,
    );

    /// Sets the clipping region by intersecting the current clip path with the given path.
    ///
    /// All subsequent drawing operations will be constrained to this new region.
    ///
    /// # Parameters
    ///
    /// - `path`: The path to use for clipping.
    /// - `mode`: The fill type to determine the clipping region.
    fn set_clip_region(&mut self, path: &PdfPath, mode: PathFillType);

    /// Returns the width of the canvas in device units.
    fn width(&self) -> f32;

    /// Returns the height of the canvas in device units.
    fn height(&self) -> f32;

    /// Resets the clipping region to the entire canvas area.
    fn reset_clip(&mut self);

    /// Draws an image onto the canvas at the current transformation.
    ///
    /// # Parameters
    ///
    /// - `image`: The image to draw.
    /// - `blend_mode`: Optional blend mode to use when compositing the image.
    fn draw_image(&mut self, image: &Image<'_>, blend_mode: Option<BlendMode>);

    /// Creates a new mask layer with the specified dimensions.
    ///
    /// # Parameters
    ///
    /// - `width`: The width of the mask layer in device units.
    /// - `height`: The height of the mask layer in device units.
    ///
    /// # Returns
    ///
    /// A boxed mask layer backend.
    fn new_mask_layer(&mut self, width: f32, height: f32) -> Box<Self::MaskType>;

    /// Begins drawing into the specified mask layer.
    ///
    /// All subsequent drawing operations will affect the mask until `end_mask_layer` is called.
    ///
    /// # Parameters
    ///
    /// - `mask`: The mask layer to begin drawing into.
    fn begin_mask_layer(&mut self, mask: &mut Self::MaskType);

    /// Ends drawing into the specified mask layer and applies it to the canvas.
    ///
    /// # Parameters
    ///
    /// - `mask`: The mask layer to end and apply.
    /// - `transform`: The transformation to apply to the mask when compositing.
    fn end_mask_layer(&mut self, mask: &mut Self::MaskType, transform: &Transform);

    /// Returns a snapshot of the current canvas as an image.
    ///
    /// # Returns
    ///
    /// An image representing the current state of the canvas.
    fn image_snapshot(&mut self) -> Image<'static>;
}
