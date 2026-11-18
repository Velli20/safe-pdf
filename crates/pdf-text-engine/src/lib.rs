//! Backend-neutral contracts for loading fonts, positioning PDF text, and rendering glyphs.
//!
#![deny(missing_docs)]

pub mod error;
mod pdf_text_decoder;
pub mod pdf_text_encoder;
mod pdf_text_host_font;
mod pdf_text_layout;
pub mod system;
pub mod text;
pub mod text_style;

pub use error::TextError;
pub use pdf_text_encoder::PdfTextEncoder;
pub use system::FontSystem;

/// Builds the standard font registry and bundled fallback policy.
#[must_use]
pub fn bundled_font_system() -> std::sync::Arc<FontSystem> {
    let mut registry = pdf_font::FontRegistry::new();
    registry.register(std::sync::Arc::new(pdf_font::type1::Type1FontDriver::new()));
    registry.register(std::sync::Arc::new(pdf_font::cff::CffFontDriver::new()));
    registry.register(std::sync::Arc::new(pdf_font::type0::Type0FontDriver::new()));
    registry.register(std::sync::Arc::new(
        pdf_font::true_type::TrueTypeFontDriver::new(),
    ));
    registry.register(std::sync::Arc::new(pdf_font::type3::Type3FontDriver::new()));
    std::sync::Arc::new(FontSystem::new(
        registry,
        std::sync::Arc::new(pdf_font::BundledFallbackProvider),
    ))
}

/// Measures encoded PDF text using the bundled font system.
#[must_use]
pub fn measure_encoded_text_width(
    spec: &pdf_font::PdfFontSpec,
    text: &[u8],
    font_size: f32,
) -> f32 {
    let system = bundled_font_system();
    let Ok(font) = system.load_pdf_font(std::sync::Arc::new(spec.clone())) else {
        return 0.0;
    };
    system.text_width(&font, text, font_size)
}
