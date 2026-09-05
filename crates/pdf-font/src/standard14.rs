//! Standard 14 PDF font identities.

use crate::flags::FontFlags;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

/// Standard 14 identity used when a PDF omits an embedded program.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Standard14Font {
    /// Times Roman.
    TimesRoman,
    /// Times bold.
    TimesBold,
    /// Times italic.
    TimesItalic,
    /// Times bold italic.
    TimesBoldItalic,
    /// Helvetica.
    #[default]
    Helvetica,
    /// Helvetica bold.
    HelveticaBold,
    /// Helvetica oblique.
    HelveticaOblique,
    /// Helvetica bold oblique.
    HelveticaBoldOblique,
    /// Courier.
    Courier,
    /// Courier bold.
    CourierBold,
    /// Courier oblique.
    CourierOblique,
    /// Courier bold oblique.
    CourierBoldOblique,
    /// Symbol.
    Symbol,
    /// Zapf Dingbats.
    ZapfDingbats,
}

impl std::fmt::Display for Standard14Font {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::TimesRoman => "Times-Roman",
            Self::TimesBold => "Times-Bold",
            Self::TimesItalic => "Times-Italic",
            Self::TimesBoldItalic => "Times-BoldItalic",
            Self::Helvetica => "Helvetica",
            Self::HelveticaBold => "Helvetica-Bold",
            Self::HelveticaOblique => "Helvetica-Oblique",
            Self::HelveticaBoldOblique => "Helvetica-BoldOblique",
            Self::Courier => "Courier",
            Self::CourierBold => "Courier-Bold",
            Self::CourierOblique => "Courier-Oblique",
            Self::CourierBoldOblique => "Courier-BoldOblique",
            Self::Symbol => "Symbol",
            Self::ZapfDingbats => "ZapfDingbats",
        };
        formatter.write_str(name)
    }
}

/// Returns the bundled TrueType substitute for a Standard 14 identity.
#[must_use]
pub fn fallback_font_bytes(font: Standard14Font) -> &'static [u8] {
    match font {
        Standard14Font::Courier => include_bytes!("../../pdf-font/assets/RobotoMono-Regular.ttf"),
        Standard14Font::CourierBold => include_bytes!("../../pdf-font/assets/RobotoMono-Bold.ttf"),
        Standard14Font::CourierOblique => {
            include_bytes!("../../pdf-font/assets/RobotoMono-Italic.ttf")
        }
        Standard14Font::CourierBoldOblique => {
            include_bytes!("../../pdf-font/assets/RobotoMono-BoldItalic.ttf")
        }
        Standard14Font::Helvetica
        | Standard14Font::Symbol
        | Standard14Font::ZapfDingbats
        | Standard14Font::TimesRoman => include_bytes!("../../pdf-font/assets/Roboto-Regular.ttf"),
        Standard14Font::HelveticaBold | Standard14Font::TimesBold => {
            include_bytes!("../../pdf-font/assets/Roboto-Bold.ttf")
        }
        Standard14Font::HelveticaOblique | Standard14Font::TimesItalic => {
            include_bytes!("../../pdf-font/assets/Roboto-Italic.ttf")
        }
        Standard14Font::HelveticaBoldOblique | Standard14Font::TimesBoldItalic => {
            include_bytes!("../../pdf-font/assets/Roboto-BoldItalic.ttf")
        }
    }
}

pub(crate) fn from_dictionary(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
    flags: FontFlags,
) -> Standard14Font {
    dictionary
        .get(b"BaseFont")
        .and_then(|value| value.try_bytes(objects).ok())
        .and_then(from_base_font_name)
        .unwrap_or_else(|| from_flags(flags))
}

/// Matches a PDF base-font name or common alias to a Standard 14 identity.
#[must_use]
pub fn from_base_font_name(name: &[u8]) -> Option<Standard14Font> {
    let name = name
        .iter()
        .position(|byte| *byte == b'+')
        .and_then(|position| name.get(position.saturating_add(1)..))
        .unwrap_or(name);
    match name {
        b"Courier" | b"CourierNew" | b"CourierNewPSMT" => Some(Standard14Font::Courier),
        b"Courier-Bold" | b"CourierNew,Bold" | b"CourierNewPS-BoldMT" => {
            Some(Standard14Font::CourierBold)
        }
        b"Courier-Oblique"
        | b"Courier-Italic"
        | b"CourierNew,Italic"
        | b"CourierNewPS-ItalicMT" => Some(Standard14Font::CourierOblique),
        b"Courier-BoldOblique"
        | b"Courier-BoldItalic"
        | b"CourierNew,BoldItalic"
        | b"CourierNewPS-BoldItalicMT" => Some(Standard14Font::CourierBoldOblique),
        b"Helvetica" | b"ArialMT" | b"Arial" => Some(Standard14Font::Helvetica),
        b"Helvetica-Bold" | b"Arial-BoldMT" | b"Arial,Bold" => Some(Standard14Font::HelveticaBold),
        b"Helvetica-Oblique" | b"Helvetica-Italic" | b"Arial-ItalicMT" | b"Arial,Italic" => {
            Some(Standard14Font::HelveticaOblique)
        }
        b"Helvetica-BoldOblique"
        | b"Helvetica-BoldItalic"
        | b"Arial-BoldItalicMT"
        | b"Arial,BoldItalic" => Some(Standard14Font::HelveticaBoldOblique),
        b"Times-Roman" | b"TimesNewRomanPSMT" | b"TimesNewRoman" | b"TimesNewRomanPS" => {
            Some(Standard14Font::TimesRoman)
        }
        b"Times-Bold" | b"TimesNewRomanPS-BoldMT" | b"TimesNewRoman,Bold" => {
            Some(Standard14Font::TimesBold)
        }
        b"Times-Italic" | b"TimesNewRomanPS-ItalicMT" | b"TimesNewRoman,Italic" => {
            Some(Standard14Font::TimesItalic)
        }
        b"Times-BoldItalic" | b"TimesNewRomanPS-BoldItalicMT" | b"TimesNewRoman,BoldItalic" => {
            Some(Standard14Font::TimesBoldItalic)
        }
        b"Symbol" | b"SymbolMT" => Some(Standard14Font::Symbol),
        b"ZapfDingbats" | b"Wingdings" | b"Wingdings-Regular" => Some(Standard14Font::ZapfDingbats),
        _ => None,
    }
}

pub(crate) fn from_flags(flags: FontFlags) -> Standard14Font {
    if flags.contains(FontFlags::SYMBOLIC) {
        return Standard14Font::Symbol;
    }
    let bold = flags.contains(FontFlags::FORCE_BOLD);
    let italic = flags.contains(FontFlags::ITALIC);
    if flags.contains(FontFlags::FIXED_PITCH) {
        return match (bold, italic) {
            (true, true) => Standard14Font::CourierBoldOblique,
            (true, false) => Standard14Font::CourierBold,
            (false, true) => Standard14Font::CourierOblique,
            (false, false) => Standard14Font::Courier,
        };
    }
    if flags.contains(FontFlags::SERIF) {
        return match (bold, italic) {
            (true, true) => Standard14Font::TimesBoldItalic,
            (true, false) => Standard14Font::TimesBold,
            (false, true) => Standard14Font::TimesItalic,
            (false, false) => Standard14Font::TimesRoman,
        };
    }
    match (bold, italic) {
        (true, true) => Standard14Font::HelveticaBoldOblique,
        (true, false) => Standard14Font::HelveticaBold,
        (false, true) => Standard14Font::HelveticaOblique,
        (false, false) => Standard14Font::Helvetica,
    }
}
