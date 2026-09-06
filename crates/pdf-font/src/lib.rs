pub mod base_encoding;
mod cff_builder;
pub mod encoding;
pub mod error;
pub mod fallback;
pub mod flags;
pub mod font;
pub mod font_registry;
pub mod glyph_name_to_unicode;
pub mod glyph_widths_map;
pub mod pdf;
mod pdf_font_descriptor;
pub mod pdf_font_handle;
mod pdf_font_metrics;
mod pdf_font_parser;
mod pdf_font_program;
pub mod pdf_font_spec;
mod pdf_path_pen;
mod query_cache;
pub mod simple_font_spec;
pub mod standard14;
pub mod text_string;
pub mod true_type;
pub mod type0;
pub mod type0_font_spec;
pub mod type1;
pub mod type3;
pub mod type3_font_spec;

pub use base_encoding::BaseEncoding;
pub use error::FontError;
pub use fallback::{
    FallbackCandidate, FallbackProvider, GlyphFallbackRequest, WholeFontFallbackRequest,
};
pub use font::{
    ExternalFontKey, FontDriver, FontFace, FontFaceId, FontLoadRequest, FontMetadata, FontMetrics,
    FontProgramFormat, FontSource, FontStretch, FontWeight, GlyphGeometry, GlyphId, GlyphName,
};
pub use font_registry::FontRegistry;
pub use pdf::{
    CidFontKind, CidFontSpec, CidSystemInfo, PDF_GLYPH_SPACE_UNITS_PER_EM, PdfCode,
    PdfFontDescriptor, PdfGlyphMetric, PdfMetrics, SimpleEncoding, SimpleFontSpec, ToUnicodeMap,
    Type0FontSpec, Type3FontSpec,
};
pub use pdf_font_spec::BundledFallbackProvider;
pub use pdf_font_spec::PdfFontSpec;
pub use standard14::Standard14Font;
