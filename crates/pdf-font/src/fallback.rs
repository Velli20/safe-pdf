use pdf_cmap::ToUnicodeCMap;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{
    cid_system_info::CidOrdering, encoding::Encoding, error::FontError, flags::FontFlags,
    simple_font_glyph_map::SimpleFontGlyphWidthsMap, standard14::Standard14Font,
    true_type_font::TrueTypeFont,
};

pub(crate) struct FallbackFontProgram {
    pub(crate) font_file: &'static [u8],
    pub(crate) standard14: Standard14Font,
    pub(crate) flags: FontFlags,
}

impl FallbackFontProgram {
    const NOTO_SANS_CJK_JP_REGULAR: &[u8] = include_bytes!("../assets/NotoSansCJKjp-Regular.otf");

    /// Select fallback font bytes and metadata for a font dictionary.
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, FontError> {
        let flags = FontFlags::from_dictionary(dictionary, objects)?;
        let standard14 = Standard14Font::from_dictionary(dictionary, objects, flags);
        let is_cjk = is_cjk_cid_font(dictionary, objects)?;
        let font_file = if is_cjk {
            Self::NOTO_SANS_CJK_JP_REGULAR
        } else {
            standard14.fallback_font_bytes()
        };

        Ok(Self {
            font_file,
            standard14,
            flags,
        })
    }
}

/// Build a synthetic TrueType font from fallback font data.
///
/// # Paramaters
///
/// - `dictionary`: The PDF font dictionary used to derive fallback metrics and metadata.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// A [`TrueTypeFont`] backed by fallback font bytes, simple font widths,
/// optional encoding, optional ToUnicode data, and descriptor flags.
pub(crate) fn fallback_true_type_from_dictionary(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<TrueTypeFont, FontError> {
    let fallback = FallbackFontProgram::from_dictionary(dictionary, objects)?;
    let widths = SimpleFontGlyphWidthsMap::from_dictionary(dictionary, objects)?;
    let encoding = Encoding::from_dictionary(dictionary, objects)
        .ok()
        .flatten();
    let to_unicode = ToUnicodeCMap::from_dictionary(dictionary, objects)?;

    Ok(TrueTypeFont {
        font_file: fallback.font_file.into(),
        widths,
        encoding,
        to_unicode,
        standard14: Some(fallback.standard14),
        flags: fallback.flags,
    })
}

/// Build a synthetic TrueType font from fallback font data without failing on
/// malformed optional metadata.
///
/// This is used by higher-level resource loading when a font resource is
/// otherwise unreadable and the parser should preserve best-effort rendering.
pub(crate) fn fallback_true_type_from_dictionary_best_effort(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> TrueTypeFont {
    let flags = FontFlags::from_dictionary(dictionary, objects).unwrap_or_default();
    let is_cjk = is_cjk_cid_font(dictionary, objects).unwrap_or(false);
    let standard14 = Standard14Font::from_dictionary(dictionary, objects, flags);
    let font_file = if is_cjk {
        FallbackFontProgram::NOTO_SANS_CJK_JP_REGULAR
    } else {
        standard14.fallback_font_bytes()
    };
    let fallback = FallbackFontProgram {
        font_file,
        standard14,
        flags,
    };
    let mut font = TrueTypeFont::from_bytes(fallback.font_file, Some(fallback.standard14));
    font.flags = fallback.flags;

    font
}

/// Detect whether a CID font dictionary uses a known CJK CID ordering.
///
/// # Paramaters
///
/// - `dictionary`: The PDF font dictionary that may contain `/CIDSystemInfo`.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// `true` when the dictionary declares a supported CJK CID ordering; otherwise
/// `false`.
fn is_cjk_cid_font(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<bool, FontError> {
    Ok(CidOrdering::from_dictionary(dictionary, objects)?.is_some())
}
