//! Loaded PDF font handles.

use std::sync::Arc;

use crate::font::FontFace;
use crate::pdf_font_spec::PdfFontSpec;

/// Loaded PDF resource retaining both PDF semantics and the selected primary face.
#[derive(Clone)]
pub struct PdfFontHandle {
    /// Shared normalized PDF semantics retained for layout and Type 3 rendering.
    spec: Arc<PdfFontSpec>,
    /// Loaded embedded face or selected whole-font substitute.
    primary: Arc<dyn FontFace>,
    /// Whether selectors must be interpreted in a substitute face's unrelated glyph space.
    uses_substitute: bool,
}

impl PdfFontHandle {
    /// Creates a handle from a normalized resource and its selected primary face.
    ///
    /// The face is assumed to represent the font program named by `spec`, so explicit glyph IDs,
    /// names, and CIDs remain valid selectors.
    #[must_use]
    pub fn new(spec: Arc<PdfFontSpec>, primary: Arc<dyn FontFace>) -> Self {
        Self {
            spec,
            primary,
            uses_substitute: false,
        }
    }

    /// Creates a handle whose selected face is a whole-font substitute.
    ///
    /// Layout records this distinction because PDF glyph identifiers do not address an unrelated
    /// substitute face; Unicode lookup must take precedence for that face.
    #[must_use]
    pub fn new_with_substitute(spec: Arc<PdfFontSpec>, primary: Arc<dyn FontFace>) -> Self {
        Self {
            spec,
            primary,
            uses_substitute: true,
        }
    }

    /// Returns the normalized PDF resource.
    #[must_use]
    pub fn spec(&self) -> &PdfFontSpec {
        self.spec.as_ref()
    }

    /// Returns the primary loaded face selected for the resource.
    ///
    /// Returning the shared handle by reference lets hot-path resolution borrow the face without an
    /// `Arc` increment for every glyph.
    #[must_use]
    pub fn primary(&self) -> &Arc<dyn FontFace> {
        &self.primary
    }

    /// Returns whether the selected face substitutes for an unusable PDF font program.
    #[must_use]
    pub const fn uses_substitute(&self) -> bool {
        self.uses_substitute
    }
}
