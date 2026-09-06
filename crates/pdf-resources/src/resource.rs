use crate::{
    external_graphics_state::ExternalGraphicsState, form::FormXObject, pattern::Pattern,
    resources::Resources,
};
use pdf_color_space::color_space::ColorSpace;
use pdf_font::PdfFontSpec;
use pdf_graphics::Image;
use pdf_object_reader::ObjectHandle;
use pdf_shading::model::Shading;
use std::sync::Arc;

/// Represents a PDF resource used on a page, such as fonts,
/// graphics states, XObjects, patterns, or shadings.
#[derive(Clone)]
pub enum Resource {
    /// A font resource used for text rendering.
    Font {
        font: Arc<PdfFontSpec>,
        /// Optional nested resources for this font, such as ExtGState or XObjects used in Type 3 fonts.
        resources: Option<ObjectHandle<Resources>>,
    },
    /// An external graphics state resource.
    ExternalGraphicsState(ObjectHandle<ExternalGraphicsState>),
    /// An image XObject resource.
    Image(Arc<Image>),
    /// An image XObject whose dimensions are malformed and cannot be rendered.
    UnavailableImage,
    /// A form XObject resource.
    Form(ObjectHandle<FormXObject>),
    /// A pattern resource, used for tiling or shading fills.
    Pattern(ObjectHandle<Pattern>),
    /// A shading resource, used for gradient fills and complex color transitions.
    Shading(Arc<Shading>),
    /// A color space resource, used for defining color models.
    ColorSpace(Arc<ColorSpace>),
}

impl Resource {
    /// Returns this resource as a font and any resources nested beneath it.
    pub fn as_font(&self) -> Option<(Arc<PdfFontSpec>, Option<Arc<Resources>>)> {
        let Self::Font { font, resources } = self else {
            return None;
        };
        Some((
            Arc::clone(font),
            resources.as_ref().map(ObjectHandle::get).transpose().ok()?,
        ))
    }

    pub(crate) fn as_external_graphics_state(&self) -> Option<Arc<ExternalGraphicsState>> {
        let Self::ExternalGraphicsState(state) = self else {
            return None;
        };
        state.get().ok()
    }

    pub(crate) fn as_pattern(&self) -> Option<Arc<Pattern>> {
        let Self::Pattern(pattern) = self else {
            return None;
        };
        pattern.get().ok()
    }

    pub(crate) fn as_shading(&self) -> Option<Arc<Shading>> {
        let Self::Shading(shading) = self else {
            return None;
        };
        Some(Arc::clone(shading))
    }

    pub(crate) fn as_color_space(&self) -> Option<Arc<ColorSpace>> {
        let Self::ColorSpace(color_space) = self else {
            return None;
        };
        Some(Arc::clone(color_space))
    }
}

impl From<Image> for Resource {
    fn from(image: Image) -> Self {
        Self::Image(Arc::new(image))
    }
}

impl From<FormXObject> for Resource {
    fn from(form: FormXObject) -> Self {
        Self::Form(ObjectHandle::from(form))
    }
}
