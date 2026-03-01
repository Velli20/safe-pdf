use crate::{
    external_graphics_state::ExternalGraphicsState, pattern::Pattern, resources::Resources,
    shading::Shading, xobject::XObject,
};
use pdf_color_space::color_space::ColorSpace;
use pdf_font::font::Font;
use std::rc::Rc;

/// Represents a PDF resource used on a page, such as fonts,
/// graphics states, XObjects, patterns, or shadings.
#[derive(Clone)]
pub enum Resource {
    /// A font resource used for text rendering.
    Font {
        font: Rc<Font>,
        /// Optional nested resources for this font, such as ExtGState or XObjects used in Type 3 fonts.
        resources: Option<Rc<Resources>>,
    },
    /// An external graphics state resource.
    ExternalGraphicsState(Rc<ExternalGraphicsState>),
    /// An XObject resource, such as an image or form object.
    XObject(Rc<XObject>),
    /// A pattern resource, used for tiling or shading fills.
    Pattern(Rc<Pattern>),
    /// A shading resource, used for gradient fills and complex color transitions.
    Shading(Rc<Shading>),
    /// A color space resource, used for defining color models.
    ColorSpace(Rc<ColorSpace>),
}
