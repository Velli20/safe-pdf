use crate::{
    external_graphics_state::ExternalGraphicsState, pattern::Pattern, resources::Resources,
    shading::Shading, xobject::XObject,
};
use pdf_color_space::color_space::ColorSpace;
use pdf_font::font::Font;
use std::{cell::OnceCell, rc::Rc};

/// Lazily-resolved handle to a resource that is still being constructed.
///
/// The resource cache inserts this handle before recursive parsing begins so
/// child lookups can preserve the entry instead of dropping it when they
/// encounter a cycle back to the same PDF object.
#[derive(Clone)]
pub struct ResourceReference {
    resource: Rc<OnceCell<Resource>>,
}

impl ResourceReference {
    pub(crate) fn new(_object_number: usize) -> Self {
        Self {
            resource: Rc::new(OnceCell::new()),
        }
    }

    pub(crate) fn resolve(&self, resource: Resource) {
        let _ = self.resource.set(resource);
    }

    pub(crate) fn resolved(&self) -> Option<&Resource> {
        self.resource.get()
    }
}

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
    /// A placeholder for a resource that is still being lazily resolved.
    CyclicReference(ResourceReference),
}

impl Resource {
    /// Creates a placeholder/reference pair for a resource object.
    ///
    /// The placeholder is inserted into the resource cache before parsing the
    /// final resource so recursive lookups can keep the entry alive until the
    /// returned [`ResourceReference`] is resolved.
    pub(crate) fn cyclic_reference(object_number: usize) -> (Self, ResourceReference) {
        let reference = ResourceReference::new(object_number);
        (Self::CyclicReference(reference.clone()), reference)
    }

    /// Returns the fully resolved resource behind `self`.
    ///
    /// When `self` is the lazy placeholder produced by
    /// [`Self::cyclic_reference`], this follows its [`ResourceReference`] and
    /// yields the published resource once parsing has completed.
    pub(crate) fn resolved(&self) -> Option<&Self> {
        match self {
            Self::CyclicReference(reference) => reference.resolved()?.resolved(),
            _ => Some(self),
        }
    }

    /// Returns this resource as a font and any resources nested beneath it.
    pub fn as_font(&self) -> Option<(&Font, Option<&Resources>)> {
        let Self::Font { font, resources } = self.resolved()? else {
            return None;
        };
        Some((font, resources.as_deref()))
    }

    pub(crate) fn as_external_graphics_state(&self) -> Option<&ExternalGraphicsState> {
        let Self::ExternalGraphicsState(state) = self.resolved()? else {
            return None;
        };
        Some(state)
    }

    pub(crate) fn as_xobject(&self) -> Option<&XObject> {
        let Self::XObject(xobject) = self.resolved()? else {
            return None;
        };
        Some(xobject)
    }

    pub(crate) fn as_pattern(&self) -> Option<&Pattern> {
        let Self::Pattern(pattern) = self.resolved()? else {
            return None;
        };
        Some(pattern)
    }

    pub(crate) fn as_shading(&self) -> Option<&Shading> {
        let Self::Shading(shading) = self.resolved()? else {
            return None;
        };
        Some(shading)
    }

    pub(crate) fn as_color_space(&self) -> Option<&ColorSpace> {
        let Self::ColorSpace(color_space) = self.resolved()? else {
            return None;
        };
        Some(color_space)
    }
}
