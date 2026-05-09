use std::borrow::Cow;
use std::ops::Deref;
use std::sync::Arc;

use pdf_graphics::{
    BlendMode, MaskMode, PathFillType, PixelFormat, color::Color, pdf_path::PdfPath, rect::Rect,
    transform::Transform,
};

use crate::{error::PdfCanvasError, recording_canvas::RecordingCanvas};

/// Image data storage that supports zero-copy sharing via `Arc`.
///
/// `ImageData` replaces `Cow<'a, [u8]>` for image pixel buffers, adding
/// a `Shared` variant backed by `Arc<[u8]>`. Once an image is recorded,
/// all subsequent clones of the recording share the same allocation
/// instead of deep-copying the pixel buffer.
#[derive(Clone)]
pub enum ImageData<'a> {
    /// Borrowed pixel data (zero-copy reference into an existing buffer).
    Borrowed(&'a [u8]),
    /// Owned pixel data (unique allocation).
    Owned(Vec<u8>),
    /// Reference-counted pixel data shared across recordings.
    Shared(Arc<[u8]>),
}

impl Deref for ImageData<'_> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            ImageData::Borrowed(b) => b,
            ImageData::Owned(v) => v,
            ImageData::Shared(a) => a,
        }
    }
}

impl ImageData<'_> {
    /// Returns a `Shared` variant that can be cheaply cloned.
    ///
    /// - `Borrowed` / `Owned`: copies the data into a new `Arc<[u8]>` once.
    /// - `Shared`: bumps the reference count (no copy).
    pub fn to_shared(&self) -> ImageData<'static> {
        match self {
            ImageData::Shared(a) => ImageData::Shared(Arc::clone(a)),
            other => ImageData::Shared(Arc::from(&**other)),
        }
    }
}

impl<'a> From<Cow<'a, [u8]>> for ImageData<'a> {
    fn from(cow: Cow<'a, [u8]>) -> Self {
        match cow {
            Cow::Borrowed(b) => ImageData::Borrowed(b),
            Cow::Owned(v) => ImageData::Owned(v),
        }
    }
}

/// Represents a shader used for advanced fill and stroke operations in PDF rendering.
///
/// A `Shader` defines how colors or patterns are applied to graphical elements, such
/// as gradients or tiling patterns. It is used to enable effects like linear gradients,
/// radial gradients, and image-based patterns when filling or stroking paths.
#[derive(Clone)]
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
        /// An optional transformation to apply to the gradient.
        ///
        /// When present, this maps the gradient's local coordinate space into device space.
        transform: Option<Transform>,
        /// The array of colors to be used in the gradient.
        colors: Cow<'a, [Color]>,
        /// The positions of each color stop, specified as values between 0.0 and 1.0.
        positions: Cow<'a, [f32]>,
    },
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
        colors: Cow<'a, [Color]>,
        /// The positions of each color stop, specified as values between 0.0 and 1.0.
        positions: Cow<'a, [f32]>,
        /// An optional transformation to apply to the gradient.
        transform: Option<Transform>,
    },
    /// A raster image shader, typically used for pre-rasterized mesh shadings.
    RasterImage {
        /// Rasterized image content.
        image: Image<'a>,
        /// Destination rect in device space.
        dest_rect: Rect,
        /// Optional local transform.
        transform: Option<Transform>,
    },
}

/// Represents an image resource for drawing or pattern tiling in the PDF canvas backend.
///
/// The `Image` struct encapsulates raw image data, dimensions, encoding, and optional
/// transformation or masking information.
#[derive(Clone)]
pub struct Image<'a> {
    /// The raw image data.
    pub data: ImageData<'a>,
    /// The width of the image in pixels.
    pub width: usize,
    /// The height of the image in pixels.
    pub height: usize,
    /// The pixel format of the image data.
    pub pixel_format: PixelFormat,
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
    /// - `shader`: An optional shader to use for the stroke.
    /// - `blend_mode`: An optional blend mode to use when stroking the path.
    fn stroke_path(
        &mut self,
        path: &PdfPath,
        color: Color,
        line_width: f32,
        shader: &Option<Shader>,
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
        image: &Image<'_>,
        blend_mode: Option<BlendMode>,
        dest_rect: Rect,
        image_rotation: Option<f32>,
    ) -> Result<(), PdfCanvasError>;

    /// Draws an inline image onto the canvas.
    ///
    /// The default implementation forwards to [`CanvasBackend::draw_image_rect`].
    fn draw_inline_image(
        &mut self,
        image: &Image<'_>,
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
