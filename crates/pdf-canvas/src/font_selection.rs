//! The font a `Tf` operand selects by resource name.
use pdf_font::{PdfFontSpec, Standard14Font};
use pdf_resources::resources::Resources;
use std::sync::Arc;

/// The font a `Tf` name selects: a font resource, or the Standard 14 font substituted
/// for a name no resources define.
#[derive(Clone)]
pub enum FontSelection {
    /// A font resource, with the nested resources of a Type 3 font.
    Resource {
        /// The resource's font specification.
        font: Arc<PdfFontSpec>,
        /// The resources a Type 3 font's glyph procedures draw with.
        resources: Option<Arc<Resources>>,
    },
    /// The Standard 14 font a name no resources define selects: the font the name names,
    /// or Helvetica, as other viewers draw it, rather than abandoning the rest of the
    /// stream.
    Standard14(Standard14Font),
}

impl FontSelection {
    /// Selects `name` from `resources`, then from `fallback` (as widget appearances find
    /// fonts their own resources lack in the interactive form's `/DR`), else the
    /// Standard 14 substitute.
    #[must_use]
    pub fn new(name: &[u8], resources: Option<&Resources>, fallback: Option<&Resources>) -> Self {
        resources
            .and_then(|resources| resources.font(name))
            .or_else(|| fallback?.font(name))
            .map_or_else(
                || Self::Standard14(Standard14Font::for_missing_resource(name)),
                |(font, resources)| Self::Resource { font, resources },
            )
    }

    /// The selected font specification; a new one for a Standard 14 substitute.
    #[must_use]
    pub fn spec(&self) -> Arc<PdfFontSpec> {
        match self {
            Self::Resource { font, .. } => Arc::clone(font),
            Self::Standard14(font) => Arc::new(PdfFontSpec::from(*font)),
        }
    }
}
