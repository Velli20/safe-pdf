pub mod color;
pub mod transform;
use num_derive::FromPrimitive;

/// Specifies the shape to be used at the end of open subpaths when they are stroked.
///
/// Corresponds to the PDF line cap style:
/// - `Butt`: The stroke ends exactly at the endpoint.
/// - `Round`: The stroke ends with a semicircular arc.
/// - `Square`: The stroke ends with a square projecting beyond the endpoint.
#[derive(Clone, Copy, PartialEq, Debug, FromPrimitive)]
pub enum LineCap {
    Butt = 0,
    Round = 1,
    Square = 2,
}

/// Specifies the shape to be used at the corners of paths when they are stroked.
///
/// Corresponds to the PDF line join style:
/// - `Miter`: Sharp corner or angled join.
/// - `Round`: Rounded join at the corner.
/// - `Bevel`: Beveled (flattened) join at the corner.
#[derive(Clone, Copy, PartialEq, Debug, FromPrimitive)]
pub enum LineJoin {
    Miter = 0,
    Round = 1,
    Bevel = 2,
}

/// Represents the standard blend modes allowed in PDF for compositing graphics.
///
/// Blend modes determine how colors from different layers are combined:
/// - `Normal`: No blending, just overlays the color.
/// - `Multiply`, `Screen`, `Overlay`, etc.: Various blending effects as defined by the PDF specification.
#[derive(Debug, PartialEq, Clone)]
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
