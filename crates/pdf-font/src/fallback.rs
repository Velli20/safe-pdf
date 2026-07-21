use std::borrow::Cow;

use pdf_cmap::ToUnicodeCMap;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

use crate::{
    cid_system_info::cid_ordering_from_dictionary,
    encoding::{Encoding, FontEncoding},
    error::FontError,
    flags::FontFlags,
    simple_font_glyph_map::SimpleFontGlyphWidthsMap,
    standard14::Standard14Font,
    true_type_font::TrueTypeFont,
};

pub(crate) struct FallbackFontProgram {
    pub(crate) font_file: Cow<'static, [u8]>,
    pub(crate) standard14: Standard14Font,
    pub(crate) flags: FontFlags,
}

/// Select fallback font bytes and metadata for a font dictionary.
///
/// # Paramaters
///
/// - `dictionary`: The PDF font dictionary that needs fallback font data.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// A fallback font program with borrowed bundled bytes, the selected Standard
/// 14 identity, and descriptor flags.
pub(crate) fn fallback_program_from_dictionary(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<FallbackFontProgram, FontError> {
    let flags = descriptor_flags(dictionary, objects)?;
    let standard14 = standard14_from_dictionary(dictionary, objects, flags);
    let is_cjk = is_cjk_cid_font(dictionary, objects)?;

    Ok(fallback_program(flags, standard14, is_cjk))
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
    let fallback = fallback_program_from_dictionary(dictionary, objects)?;
    let widths = SimpleFontGlyphWidthsMap::from_dictionary(dictionary, objects)?;
    let encoding = simple_font_encoding(dictionary, objects);
    let to_unicode = to_unicode_cmap(dictionary, objects)?;

    Ok(TrueTypeFont {
        font_file: fallback.font_file,
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
    let flags = descriptor_flags(dictionary, objects).unwrap_or_default();
    let is_cjk = is_cjk_cid_font(dictionary, objects).unwrap_or(false);
    let standard14 = standard14_from_dictionary(dictionary, objects, flags);
    let fallback = fallback_program(flags, standard14, is_cjk);
    let mut font = TrueTypeFont::from_bytes(fallback.font_file, Some(fallback.standard14));
    font.flags = fallback.flags;

    font
}

/// Resolve the Standard 14 identity to use for fallback substitution.
///
/// This prefers a readable `/BaseFont` name when it maps to a known Standard 14
/// font. If `/BaseFont` is missing, malformed, or unrecognized, it falls back
/// to the flag-driven Standard 14 selection.
fn standard14_from_dictionary(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
    flags: FontFlags,
) -> Standard14Font {
    dictionary
        .get("BaseFont")
        .and_then(|value| value.try_str(objects).ok())
        .and_then(Standard14Font::from_base_font_name)
        .unwrap_or_else(|| Standard14Font::from(flags))
}

/// Build the fallback font program descriptor from already-decided inputs.
///
/// `standard14` selects the Standard 14 identity for simple-font fallback,
/// while `is_cjk` switches the program bytes to the bundled CJK fallback for
/// CID fonts that declare a supported CJK ordering.
fn fallback_program(
    flags: FontFlags,
    standard14: Standard14Font,
    is_cjk: bool,
) -> FallbackFontProgram {
    let font_file: Cow<'static, [u8]> = if is_cjk {
        Cow::Borrowed(include_bytes!("../assets/NotoSansCJKjp-Regular.otf").as_slice())
    } else {
        standard14.fallback_font_bytes()
    };

    FallbackFontProgram {
        font_file,
        standard14,
        flags,
    }
}

/// Read font descriptor flags from a font dictionary.
///
/// # Paramaters
///
/// - `dictionary`: The PDF font dictionary that may contain `/FontDescriptor`.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// The parsed descriptor flags, or empty flags when the descriptor or `/Flags`
/// entry is absent.
pub(crate) fn descriptor_flags(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<FontFlags, FontError> {
    let Some(descriptor) = dictionary.optional_dictionary("FontDescriptor", objects)? else {
        return Ok(FontFlags::empty());
    };

    Ok(descriptor
        .get("Flags")
        .and_then(|obj| obj.try_number::<u32>(objects).ok())
        .map(FontFlags::from_bits_truncate)
        .unwrap_or_default())
}

/// Parse a simple font encoding from a font dictionary.
///
/// # Paramaters
///
/// - `dictionary`: The PDF font dictionary that may contain `/Encoding`.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// The parsed encoding when `/Encoding` is present and supported.
fn simple_font_encoding(dictionary: &Dictionary, objects: &dyn ObjectResolver) -> Option<Encoding> {
    dictionary.get("Encoding").and_then(|enc_obj| {
        let resolved = objects.resolve_object(enc_obj).ok()?;
        match resolved {
            ObjectVariant::Dictionary(d) => Encoding::from_dictionary(d, objects).ok(),
            _ => {
                let base = FontEncoding::from(resolved.try_str(objects).ok()?);
                Encoding::from_base_encoding(base).ok()
            }
        }
    })
}

/// Parse an optional ToUnicode CMap from a font dictionary.
///
/// # Paramaters
///
/// - `dictionary`: The PDF font dictionary that may contain `/ToUnicode`.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// The parsed ToUnicode CMap when a readable `/ToUnicode` stream is present.
fn to_unicode_cmap(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Option<ToUnicodeCMap>, FontError> {
    dictionary
        .get("ToUnicode")
        .and_then(|e| e.try_stream(objects).ok())
        .map(|s| ToUnicodeCMap::try_from(s.raw_data()))
        .transpose()
        .map_err(FontError::from)
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
    Ok(cid_ordering_from_dictionary(dictionary, objects)?.is_some())
}
