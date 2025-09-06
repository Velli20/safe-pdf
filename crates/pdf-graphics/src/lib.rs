pub mod color;
pub mod pdf_path;
pub mod transform;
use num_derive::FromPrimitive;

/// Specifies the shape to be used at the end of open subpaths when they are stroked.
#[derive(Clone, Copy, PartialEq, Debug, FromPrimitive)]
pub enum LineCap {
    /// The stroke ends exactly at the endpoint.
    Butt = 0,
    /// The stroke ends with a semicircular arc.
    Round = 1,
    /// The stroke ends with a square projecting beyond the endpoint.
    Square = 2,
}

/// Specifies the shape to be used at the corners of paths when they are stroked.
#[derive(Clone, Copy, PartialEq, Debug, FromPrimitive)]
pub enum LineJoin {
    /// Sharp corner or angled join.
    Miter = 0,
    /// Rounded join at the corner.
    Round = 1,
    /// Beveled (flattened) join at the corner.
    Bevel = 2,
}

/// Represents the standard blend modes allowed in PDF for compositing graphics.
///
/// Blend modes determine how colors from different layers are combined:
/// - `Normal`: No blending, just overlays the color.
/// - `Multiply`, `Screen`, `Overlay`, etc.: Various blending effects as defined by the PDF specification.
#[derive(PartialEq, Clone, Copy)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

/// Specifies how a path should be painted in PDF graphics operations.
#[derive(Default, Clone, PartialEq)]
pub enum PaintMode {
    /// Fill the interior of the path.
    #[default]
    Fill,
    /// Stroke the outline of the path.
    Stroke,
    /// Fill the interior and stroke the outline of the path.
    FillAndStroke,
}

/// Determines the rule used to define the "inside" region of a path for filling operations.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum PathFillType {
    /// Non-zero winding number rule: "inside" is computed by a non-zero sum of signed edge crossings.
    #[default]
    Winding,
    /// Even-odd rule: "inside" is computed by an odd number of edge crossings.
    EvenOdd,
}
