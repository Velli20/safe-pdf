use crate::{
    external_graphics_state::ExternalGraphicsState, pattern::Pattern, shading::Shading,
    xobject::XObject,
};
use pdf_font::font::Font;
use std::rc::Rc;

/// Represents a PDF resource used on a page, such as fonts,
/// graphics states, XObjects, patterns, or shadings.
#[derive(Clone)]
pub enum Resource {
    /// A font resource used for text rendering.
    Font(Rc<Font>),
    /// An external graphics state resource.
    ExternalGraphicsState(Rc<ExternalGraphicsState>),
    /// An XObject resource, such as an image or form object.
    XObject(Rc<XObject>),
    /// A pattern resource, used for tiling or shading fills.
    Pattern(Rc<Pattern>),
    /// A shading resource, used for gradient fills and complex color transitions.
    Shading(Rc<Shading>),
}
