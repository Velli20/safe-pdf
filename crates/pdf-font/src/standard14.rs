/// Detection and fallback font selection for the PDF Standard 14 fonts.
///
/// The PDF spec guarantees that viewers can render these 14 Type 1 fonts
/// without an embedded font program.  When a document references one by
/// name alone we substitute a metrically-similar bundled TrueType font.
use std::fmt;

use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::flags::FontFlags;

/// One of the 14 standard Type 1 fonts defined by the PDF specification
/// (ISO 32000-1 §9.6.2.2).
///
/// These fonts are guaranteed to be available in every conforming PDF viewer.
/// When a document references one without embedding a font program, the
/// renderer substitutes a bundled TrueType font with similar metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standard14Font {
    /// Courier — monospaced, regular weight.
    Courier,
    /// Courier-Bold — monospaced, bold weight.
    CourierBold,
    /// Courier-Oblique — monospaced, italic.
    CourierOblique,
    /// Courier-BoldOblique — monospaced, bold italic.
    CourierBoldOblique,
    /// Helvetica — proportional sans-serif, regular weight. Default fallback.
    Helvetica,
    /// Helvetica-Bold — proportional sans-serif, bold weight.
    HelveticaBold,
    /// Helvetica-Oblique — proportional sans-serif, italic.
    HelveticaOblique,
    /// Helvetica-BoldOblique — proportional sans-serif, bold italic.
    HelveticaBoldOblique,
    /// Times-Roman — proportional serif, regular weight.
    TimesRoman,
    /// Times-Bold — proportional serif, bold weight.
    TimesBold,
    /// Times-Italic — proportional serif, italic.
    TimesItalic,
    /// Times-BoldItalic — proportional serif, bold italic.
    TimesBoldItalic,
    /// Symbol — symbolic character set.
    Symbol,
    /// ZapfDingbats — decorative symbols.
    ZapfDingbats,
}

/// The default Standard 14 font is Helvetica (proportional sans-serif, regular).
impl Default for Standard14Font {
    fn default() -> Self {
        Self::Helvetica
    }
}

/// Displays the canonical PDF `/BaseFont` name (e.g. `"Courier-Bold"`).
impl fmt::Display for Standard14Font {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Courier => "Courier",
            Self::CourierBold => "Courier-Bold",
            Self::CourierOblique => "Courier-Oblique",
            Self::CourierBoldOblique => "Courier-BoldOblique",
            Self::Helvetica => "Helvetica",
            Self::HelveticaBold => "Helvetica-Bold",
            Self::HelveticaOblique => "Helvetica-Oblique",
            Self::HelveticaBoldOblique => "Helvetica-BoldOblique",
            Self::TimesRoman => "Times-Roman",
            Self::TimesBold => "Times-Bold",
            Self::TimesItalic => "Times-Italic",
            Self::TimesBoldItalic => "Times-BoldItalic",
            Self::Symbol => "Symbol",
            Self::ZapfDingbats => "ZapfDingbats",
        };
        f.write_str(name)
    }
}

/// Selects a Standard 14 font from font descriptor flags.
///
/// Priority (per ISO 32000-1 §9.6.2.2 and §9.8):
///   1. **Symbolic** → [`Standard14Font::Symbol`]
///   2. **FixedPitch** → Courier family
///   3. **Serif** → Times family
///   4. Otherwise → Helvetica family
///
/// Within each family, `ForceBold` and `Italic` select the weight/style variant.
impl From<FontFlags> for Standard14Font {
    fn from(flags: FontFlags) -> Self {
        if flags.contains(FontFlags::SYMBOLIC) {
            return Self::Symbol;
        }

        let is_bold = flags.contains(FontFlags::FORCE_BOLD);
        let is_italic = flags.contains(FontFlags::ITALIC);

        if flags.contains(FontFlags::FIXED_PITCH) {
            match (is_bold, is_italic) {
                (false, false) => Self::Courier,
                (true, false) => Self::CourierBold,
                (false, true) => Self::CourierOblique,
                (true, true) => Self::CourierBoldOblique,
            }
        } else if flags.contains(FontFlags::SERIF) {
            match (is_bold, is_italic) {
                (false, false) => Self::TimesRoman,
                (true, false) => Self::TimesBold,
                (false, true) => Self::TimesItalic,
                (true, true) => Self::TimesBoldItalic,
            }
        } else {
            match (is_bold, is_italic) {
                (false, false) => Self::Helvetica,
                (true, false) => Self::HelveticaBold,
                (false, true) => Self::HelveticaOblique,
                (true, true) => Self::HelveticaBoldOblique,
            }
        }
    }
}

impl Standard14Font {
    /// Resolve the Standard 14 identity to use for fallback substitution.
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        flags: FontFlags,
    ) -> Self {
        dictionary
            .get(b"BaseFont")
            .and_then(|value| value.try_bytes(objects).ok())
            .and_then(Self::from_base_font_name)
            .unwrap_or_else(|| Self::from(flags))
    }

    /// Try to match a `/BaseFont` name to a Standard 14 font.
    ///
    /// Recognises both the canonical names and common aliases
    /// (e.g. "TimesNewRomanPSMT" maps to `TimesRoman`).
    pub fn from_base_font_name(name: &[u8]) -> Option<Self> {
        // Strip a subset prefix like "ABCDEF+" that some PDF producers add.
        let name = name
            .iter()
            .position(|byte| *byte == b'+')
            .map_or(name, |pos| {
                name.get(pos.saturating_add(1)..).unwrap_or(name)
            });

        match name {
            b"Courier" | b"CourierNew" | b"CourierNewPSMT" => Some(Self::Courier),
            b"Courier-Bold" | b"CourierNew,Bold" | b"CourierNewPS-BoldMT" => {
                Some(Self::CourierBold)
            }
            b"Courier-Oblique"
            | b"Courier-Italic"
            | b"CourierNew,Italic"
            | b"CourierNewPS-ItalicMT" => Some(Self::CourierOblique),
            b"Courier-BoldOblique"
            | b"Courier-BoldItalic"
            | b"CourierNew,BoldItalic"
            | b"CourierNewPS-BoldItalicMT" => Some(Self::CourierBoldOblique),

            b"Helvetica" | b"ArialMT" | b"Arial" => Some(Self::Helvetica),
            b"Helvetica-Bold" | b"Arial-BoldMT" | b"Arial,Bold" => Some(Self::HelveticaBold),
            b"Helvetica-Oblique" | b"Helvetica-Italic" | b"Arial-ItalicMT" | b"Arial,Italic" => {
                Some(Self::HelveticaOblique)
            }
            b"Helvetica-BoldOblique"
            | b"Helvetica-BoldItalic"
            | b"Arial-BoldItalicMT"
            | b"Arial,BoldItalic" => Some(Self::HelveticaBoldOblique),

            b"Times-Roman" | b"TimesNewRomanPSMT" | b"TimesNewRoman" | b"TimesNewRomanPS" => {
                Some(Self::TimesRoman)
            }
            b"Times-Bold" | b"TimesNewRomanPS-BoldMT" | b"TimesNewRoman,Bold" => {
                Some(Self::TimesBold)
            }
            b"Times-Italic" | b"TimesNewRomanPS-ItalicMT" | b"TimesNewRoman,Italic" => {
                Some(Self::TimesItalic)
            }
            b"Times-BoldItalic" | b"TimesNewRomanPS-BoldItalicMT" | b"TimesNewRoman,BoldItalic" => {
                Some(Self::TimesBoldItalic)
            }

            b"Symbol" | b"SymbolMT" => Some(Self::Symbol),
            b"ZapfDingbats" | b"Wingdings" | b"Wingdings-Regular" => Some(Self::ZapfDingbats),

            _ => None,
        }
    }

    /// Return bundled TrueType font bytes that serve as a visual substitute.
    pub fn fallback_font_bytes(&self) -> &'static [u8] {
        match self {
            Self::Courier => include_bytes!("../assets/RobotoMono-Regular.ttf"),
            Self::CourierBold => include_bytes!("../assets/RobotoMono-Bold.ttf"),
            Self::CourierOblique => include_bytes!("../assets/RobotoMono-Italic.ttf"),
            Self::CourierBoldOblique => include_bytes!("../assets/RobotoMono-BoldItalic.ttf"),

            Self::Helvetica | Self::Symbol | Self::ZapfDingbats => {
                include_bytes!("../assets/Roboto-Regular.ttf")
            }
            Self::HelveticaBold => include_bytes!("../assets/Roboto-Bold.ttf"),
            Self::HelveticaOblique => include_bytes!("../assets/Roboto-Italic.ttf"),
            Self::HelveticaBoldOblique => include_bytes!("../assets/Roboto-BoldItalic.ttf"),

            // Times maps to the same sans-serif substitute (no bundled serif font).
            Self::TimesRoman => include_bytes!("../assets/Roboto-Regular.ttf"),
            Self::TimesBold => include_bytes!("../assets/Roboto-Bold.ttf"),
            Self::TimesItalic => include_bytes!("../assets/Roboto-Italic.ttf"),
            Self::TimesBoldItalic => include_bytes!("../assets/Roboto-BoldItalic.ttf"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every canonical Standard 14 name must be recognised.
    #[test]
    fn canonical_names() {
        let cases: [(&[u8], Standard14Font); 14] = [
            (b"Courier", Standard14Font::Courier),
            (b"Courier-Bold", Standard14Font::CourierBold),
            (b"Courier-Oblique", Standard14Font::CourierOblique),
            (b"Courier-BoldOblique", Standard14Font::CourierBoldOblique),
            (b"Helvetica", Standard14Font::Helvetica),
            (b"Helvetica-Bold", Standard14Font::HelveticaBold),
            (b"Helvetica-Oblique", Standard14Font::HelveticaOblique),
            (
                b"Helvetica-BoldOblique",
                Standard14Font::HelveticaBoldOblique,
            ),
            (b"Times-Roman", Standard14Font::TimesRoman),
            (b"Times-Bold", Standard14Font::TimesBold),
            (b"Times-Italic", Standard14Font::TimesItalic),
            (b"Times-BoldItalic", Standard14Font::TimesBoldItalic),
            (b"Symbol", Standard14Font::Symbol),
            (b"ZapfDingbats", Standard14Font::ZapfDingbats),
        ];
        for (name, expected) in cases {
            assert_eq!(
                Standard14Font::from_base_font_name(name),
                Some(expected),
                "failed for {name:?}"
            );
        }
    }

    /// Common aliases should resolve to the correct variant.
    #[test]
    fn common_aliases() {
        assert_eq!(
            Standard14Font::from_base_font_name(b"ArialMT"),
            Some(Standard14Font::Helvetica)
        );
        assert_eq!(
            Standard14Font::from_base_font_name(b"TimesNewRomanPSMT"),
            Some(Standard14Font::TimesRoman)
        );
        assert_eq!(
            Standard14Font::from_base_font_name(b"CourierNewPSMT"),
            Some(Standard14Font::Courier)
        );
    }

    /// Subset-prefixed names (e.g. "ABCDEF+Helvetica") should still match.
    #[test]
    fn subset_prefix() {
        assert_eq!(
            Standard14Font::from_base_font_name(b"ABCDEF+Helvetica"),
            Some(Standard14Font::Helvetica)
        );
        assert_eq!(
            Standard14Font::from_base_font_name(b"GHIJKL+Courier-Bold"),
            Some(Standard14Font::CourierBold)
        );
    }

    /// Unknown font names must return `None`.
    #[test]
    fn unknown_name() {
        assert_eq!(Standard14Font::from_base_font_name(b"FooBarFont"), None);
        assert_eq!(Standard14Font::from_base_font_name(b""), None);
    }

    /// Every variant must return non-empty fallback bytes.
    #[test]
    fn fallback_bytes_non_empty() {
        let all = [
            Standard14Font::Courier,
            Standard14Font::CourierBold,
            Standard14Font::CourierOblique,
            Standard14Font::CourierBoldOblique,
            Standard14Font::Helvetica,
            Standard14Font::HelveticaBold,
            Standard14Font::HelveticaOblique,
            Standard14Font::HelveticaBoldOblique,
            Standard14Font::TimesRoman,
            Standard14Font::TimesBold,
            Standard14Font::TimesItalic,
            Standard14Font::TimesBoldItalic,
            Standard14Font::Symbol,
            Standard14Font::ZapfDingbats,
        ];
        for variant in all {
            assert!(
                !variant.fallback_font_bytes().is_empty(),
                "{variant:?} returned empty bytes"
            );
        }
    }

    // ---- From<FontFlags> tests ----

    /// No flags set → default sans-serif (Helvetica).
    #[test]
    fn from_flags_empty() {
        assert_eq!(
            Standard14Font::from(FontFlags::empty()),
            Standard14Font::Helvetica,
        );
    }

    /// SERIF flag → Times-Roman family.
    #[test]
    fn from_flags_serif() {
        assert_eq!(
            Standard14Font::from(FontFlags::SERIF),
            Standard14Font::TimesRoman,
        );
    }

    /// SERIF + ITALIC + FORCE_BOLD → TimesBoldItalic.
    #[test]
    fn from_flags_serif_bold_italic() {
        let flags = FontFlags::SERIF | FontFlags::ITALIC | FontFlags::FORCE_BOLD;
        assert_eq!(Standard14Font::from(flags), Standard14Font::TimesBoldItalic,);
    }

    /// SYMBOLIC takes priority over everything else.
    #[test]
    fn from_flags_symbolic() {
        assert_eq!(
            Standard14Font::from(FontFlags::SYMBOLIC),
            Standard14Font::Symbol,
        );
    }

    /// SYMBOLIC + SERIF → Symbol (symbolic wins).
    #[test]
    fn from_flags_symbolic_plus_serif() {
        let flags = FontFlags::SYMBOLIC | FontFlags::SERIF;
        assert_eq!(Standard14Font::from(flags), Standard14Font::Symbol);
    }

    /// Contradictory SYMBOLIC + NON_SYMBOLIC → Symbol (symbolic priority).
    #[test]
    fn from_flags_contradictory_symbolic() {
        let flags = FontFlags::SYMBOLIC | FontFlags::NON_SYMBOLIC;
        assert_eq!(Standard14Font::from(flags), Standard14Font::Symbol);
    }

    /// FIXED_PITCH → Courier family.
    #[test]
    fn from_flags_fixed_pitch() {
        assert_eq!(
            Standard14Font::from(FontFlags::FIXED_PITCH),
            Standard14Font::Courier,
        );
    }

    /// FIXED_PITCH + FORCE_BOLD + ITALIC → CourierBoldOblique.
    #[test]
    fn from_flags_fixed_bold_italic() {
        let flags = FontFlags::FIXED_PITCH | FontFlags::FORCE_BOLD | FontFlags::ITALIC;
        assert_eq!(
            Standard14Font::from(flags),
            Standard14Font::CourierBoldOblique,
        );
    }

    /// FIXED_PITCH + SERIF → Courier (fixed pitch wins over serif).
    #[test]
    fn from_flags_fixed_pitch_plus_serif() {
        let flags = FontFlags::FIXED_PITCH | FontFlags::SERIF;
        assert_eq!(Standard14Font::from(flags), Standard14Font::Courier);
    }

    /// FORCE_BOLD alone → HelveticaBold (sans-serif default).
    #[test]
    fn from_flags_bold_only() {
        assert_eq!(
            Standard14Font::from(FontFlags::FORCE_BOLD),
            Standard14Font::HelveticaBold,
        );
    }

    /// ITALIC alone → HelveticaOblique (sans-serif default).
    #[test]
    fn from_flags_italic_only() {
        assert_eq!(
            Standard14Font::from(FontFlags::ITALIC),
            Standard14Font::HelveticaOblique,
        );
    }

    /// SERIF + ITALIC → TimesItalic.
    #[test]
    fn from_flags_serif_italic() {
        let flags = FontFlags::SERIF | FontFlags::ITALIC;
        assert_eq!(Standard14Font::from(flags), Standard14Font::TimesItalic,);
    }

    /// SERIF + FORCE_BOLD → TimesBold.
    #[test]
    fn from_flags_serif_bold() {
        let flags = FontFlags::SERIF | FontFlags::FORCE_BOLD;
        assert_eq!(Standard14Font::from(flags), Standard14Font::TimesBold);
    }

    /// Verify FontFlags bit positions match the PDF spec (ISO 32000-1, Table 123).
    #[test]
    fn flag_bit_positions_match_spec() {
        // PDF spec uses 1-based bit positions.
        assert_eq!(FontFlags::FIXED_PITCH.bits(), 1 << 0); // bit 1
        assert_eq!(FontFlags::SERIF.bits(), 1 << 1); // bit 2
        assert_eq!(FontFlags::SYMBOLIC.bits(), 1 << 2); // bit 3
        assert_eq!(FontFlags::SCRIPT.bits(), 1 << 3); // bit 4
        assert_eq!(FontFlags::NON_SYMBOLIC.bits(), 1 << 5); // bit 6
        assert_eq!(FontFlags::ITALIC.bits(), 1 << 6); // bit 7
        assert_eq!(FontFlags::ALL_CAP.bits(), 1 << 16); // bit 17
        assert_eq!(FontFlags::SMALL_CAP.bits(), 1 << 17); // bit 18
        assert_eq!(FontFlags::FORCE_BOLD.bits(), 1 << 18); // bit 19
    }

    /// Round-trip: raw PDF integer → FontFlags → `From` produces correct result.
    /// Simulates a real PDF with Flags = 0x42 (Serif + Italic in spec positions).
    #[test]
    fn from_flags_raw_pdf_integer() {
        // 0x42 = bit 2 (Serif) + bit 7 (Italic) in 1-based spec positions
        //      = (1 << 1) | (1 << 6) = 2 + 64 = 66
        let raw: u32 = 0x42;
        let flags = FontFlags::from_bits_truncate(raw);
        assert!(flags.contains(FontFlags::SERIF));
        assert!(flags.contains(FontFlags::ITALIC));
        assert_eq!(Standard14Font::from(flags), Standard14Font::TimesItalic,);
    }

    // ---- Default trait ----

    /// Default is Helvetica (proportional sans-serif, regular).
    #[test]
    fn default_is_helvetica() {
        assert_eq!(Standard14Font::default(), Standard14Font::Helvetica);
    }

    // ---- Display trait ----

    /// Display returns the canonical PDF `/BaseFont` name.
    #[test]
    fn display_canonical_names() {
        assert_eq!(Standard14Font::Courier.to_string(), "Courier");
        assert_eq!(Standard14Font::CourierBold.to_string(), "Courier-Bold");
        assert_eq!(Standard14Font::TimesRoman.to_string(), "Times-Roman");
        assert_eq!(Standard14Font::ZapfDingbats.to_string(), "ZapfDingbats");
        assert_eq!(
            Standard14Font::HelveticaBoldOblique.to_string(),
            "Helvetica-BoldOblique"
        );
    }
}
