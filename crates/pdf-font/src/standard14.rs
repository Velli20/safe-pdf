//! Standard 14 PDF font identities.

use crate::flags::FontFlags;

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

/// Selects a Standard 14 identity from a best-effort BaseFont hint and fallback flags.
pub(crate) fn from_context(
    context: &mut pdf_object_reader::DictionaryContext<
        '_,
        impl pdf_object_reader::ObjectAccess + ?Sized,
    >,
    flags: FontFlags,
) -> Standard14Font {
    context
        .optional::<std::sync::Arc<[u8]>>(b"BaseFont")
        .ok()
        .flatten()
        .as_deref()
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

impl Standard14Font {
    /// Returns the advance width, in 1/1000 em, of a named glyph in this font's AFM metrics.
    ///
    /// Only Zapf Dingbats is tabulated, since its glyphs have no Latin substitute with
    /// comparable widths; other fonts return `None`.
    #[must_use]
    pub fn glyph_width(self, name: &[u8]) -> Option<f32> {
        let table = match self {
            Self::ZapfDingbats => ZAPF_DINGBATS_WIDTHS,
            _ => return None,
        };
        table
            .binary_search_by_key(&name, |&(glyph, _)| glyph)
            .ok()
            .and_then(|index| table.get(index))
            .map(|&(_, width)| width)
    }
}

/// Glyph widths from the Adobe Zapf Dingbats AFM, sorted by name in byte order.
const ZAPF_DINGBATS_WIDTHS: &[(&[u8], f32)] = &[
    (b"a1", 974.0),
    (b"a10", 692.0),
    (b"a100", 668.0),
    (b"a101", 732.0),
    (b"a102", 544.0),
    (b"a103", 544.0),
    (b"a104", 910.0),
    (b"a105", 911.0),
    (b"a106", 667.0),
    (b"a107", 760.0),
    (b"a108", 760.0),
    (b"a109", 626.0),
    (b"a11", 960.0),
    (b"a110", 694.0),
    (b"a111", 595.0),
    (b"a112", 776.0),
    (b"a117", 690.0),
    (b"a118", 791.0),
    (b"a119", 790.0),
    (b"a12", 939.0),
    (b"a120", 788.0),
    (b"a121", 788.0),
    (b"a122", 788.0),
    (b"a123", 788.0),
    (b"a124", 788.0),
    (b"a125", 788.0),
    (b"a126", 788.0),
    (b"a127", 788.0),
    (b"a128", 788.0),
    (b"a129", 788.0),
    (b"a13", 549.0),
    (b"a130", 788.0),
    (b"a131", 788.0),
    (b"a132", 788.0),
    (b"a133", 788.0),
    (b"a134", 788.0),
    (b"a135", 788.0),
    (b"a136", 788.0),
    (b"a137", 788.0),
    (b"a138", 788.0),
    (b"a139", 788.0),
    (b"a14", 855.0),
    (b"a140", 788.0),
    (b"a141", 788.0),
    (b"a142", 788.0),
    (b"a143", 788.0),
    (b"a144", 788.0),
    (b"a145", 788.0),
    (b"a146", 788.0),
    (b"a147", 788.0),
    (b"a148", 788.0),
    (b"a149", 788.0),
    (b"a15", 911.0),
    (b"a150", 788.0),
    (b"a151", 788.0),
    (b"a152", 788.0),
    (b"a153", 788.0),
    (b"a154", 788.0),
    (b"a155", 788.0),
    (b"a156", 788.0),
    (b"a157", 788.0),
    (b"a158", 788.0),
    (b"a159", 788.0),
    (b"a16", 933.0),
    (b"a160", 894.0),
    (b"a161", 838.0),
    (b"a162", 924.0),
    (b"a163", 1016.0),
    (b"a164", 458.0),
    (b"a165", 924.0),
    (b"a166", 918.0),
    (b"a167", 927.0),
    (b"a168", 928.0),
    (b"a169", 928.0),
    (b"a17", 945.0),
    (b"a170", 834.0),
    (b"a171", 873.0),
    (b"a172", 828.0),
    (b"a173", 924.0),
    (b"a174", 917.0),
    (b"a175", 930.0),
    (b"a176", 931.0),
    (b"a177", 463.0),
    (b"a178", 883.0),
    (b"a179", 836.0),
    (b"a18", 974.0),
    (b"a180", 867.0),
    (b"a181", 696.0),
    (b"a182", 874.0),
    (b"a183", 760.0),
    (b"a184", 946.0),
    (b"a185", 865.0),
    (b"a186", 967.0),
    (b"a187", 831.0),
    (b"a188", 873.0),
    (b"a189", 927.0),
    (b"a19", 755.0),
    (b"a190", 970.0),
    (b"a191", 918.0),
    (b"a192", 748.0),
    (b"a193", 836.0),
    (b"a194", 771.0),
    (b"a195", 888.0),
    (b"a196", 748.0),
    (b"a197", 771.0),
    (b"a198", 888.0),
    (b"a199", 867.0),
    (b"a2", 961.0),
    (b"a20", 846.0),
    (b"a200", 696.0),
    (b"a201", 874.0),
    (b"a202", 974.0),
    (b"a203", 762.0),
    (b"a204", 759.0),
    (b"a205", 509.0),
    (b"a206", 410.0),
    (b"a21", 762.0),
    (b"a22", 761.0),
    (b"a23", 571.0),
    (b"a24", 677.0),
    (b"a25", 763.0),
    (b"a26", 760.0),
    (b"a27", 759.0),
    (b"a28", 754.0),
    (b"a29", 786.0),
    (b"a3", 980.0),
    (b"a30", 788.0),
    (b"a31", 788.0),
    (b"a32", 790.0),
    (b"a33", 793.0),
    (b"a34", 794.0),
    (b"a35", 816.0),
    (b"a36", 823.0),
    (b"a37", 789.0),
    (b"a38", 841.0),
    (b"a39", 823.0),
    (b"a4", 719.0),
    (b"a40", 833.0),
    (b"a41", 816.0),
    (b"a42", 831.0),
    (b"a43", 923.0),
    (b"a44", 744.0),
    (b"a45", 723.0),
    (b"a46", 749.0),
    (b"a47", 790.0),
    (b"a48", 792.0),
    (b"a49", 695.0),
    (b"a5", 789.0),
    (b"a50", 776.0),
    (b"a51", 768.0),
    (b"a52", 792.0),
    (b"a53", 759.0),
    (b"a54", 707.0),
    (b"a55", 708.0),
    (b"a56", 682.0),
    (b"a57", 701.0),
    (b"a58", 826.0),
    (b"a59", 815.0),
    (b"a6", 494.0),
    (b"a60", 789.0),
    (b"a61", 789.0),
    (b"a62", 707.0),
    (b"a63", 687.0),
    (b"a64", 696.0),
    (b"a65", 689.0),
    (b"a66", 786.0),
    (b"a67", 787.0),
    (b"a68", 713.0),
    (b"a69", 791.0),
    (b"a7", 552.0),
    (b"a70", 785.0),
    (b"a71", 791.0),
    (b"a72", 873.0),
    (b"a73", 761.0),
    (b"a74", 762.0),
    (b"a75", 759.0),
    (b"a76", 892.0),
    (b"a77", 892.0),
    (b"a78", 788.0),
    (b"a79", 784.0),
    (b"a8", 537.0),
    (b"a81", 438.0),
    (b"a82", 138.0),
    (b"a83", 277.0),
    (b"a84", 415.0),
    (b"a85", 509.0),
    (b"a86", 410.0),
    (b"a87", 234.0),
    (b"a88", 234.0),
    (b"a89", 390.0),
    (b"a9", 577.0),
    (b"a90", 390.0),
    (b"a91", 276.0),
    (b"a92", 276.0),
    (b"a93", 317.0),
    (b"a94", 317.0),
    (b"a95", 334.0),
    (b"a96", 334.0),
    (b"a97", 392.0),
    (b"a98", 392.0),
    (b"a99", 668.0),
    (b"space", 278.0),
];
