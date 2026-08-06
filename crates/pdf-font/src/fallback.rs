use pdf_cmap::ToUnicodeCMap;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{
    cid_system_info::CidOrdering, encoding::Encoding, flags::FontFlags,
    simple_font_glyph_map::SimpleFontGlyphWidthsMap, standard14::Standard14Font,
    true_type_font::TrueTypeFont,
};

const NOTO_SANS_CJK_JP_REGULAR: &[u8] = include_bytes!("../assets/NotoSansCJKjp-Regular.otf");

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
/// optional encoding, optional ToUnicode data, and descriptor flags. Each
/// metadata field is parsed independently and ignored when malformed.
pub(crate) fn fallback_true_type_from_dictionary(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> TrueTypeFont {
    let flags = FontFlags::from_dictionary(dictionary, objects).unwrap_or_default();
    let standard14 = Standard14Font::from_dictionary(dictionary, objects, flags);
    let font_file = if is_cjk_cid_font(dictionary, objects) {
        NOTO_SANS_CJK_JP_REGULAR
    } else {
        standard14.fallback_font_bytes()
    };
    let widths = SimpleFontGlyphWidthsMap::from_dictionary(dictionary, objects)
        .ok()
        .flatten();
    let encoding = Encoding::from_dictionary(dictionary, objects)
        .ok()
        .flatten();
    let to_unicode = ToUnicodeCMap::from_dictionary(dictionary, objects)
        .ok()
        .flatten();
    let mut font = TrueTypeFont::from_bytes(font_file, Some(standard14));
    font.widths = widths;
    if encoding.is_some() {
        font.encoding = encoding;
    }
    font.to_unicode = to_unicode;
    font.flags = flags;

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
fn is_cjk_cid_font(dictionary: &Dictionary, objects: &dyn ObjectResolver) -> bool {
    CidOrdering::from_dictionary(dictionary, objects)
        .ok()
        .flatten()
        .is_some()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object::{
        object_resolver::PassthroughResolver, object_variant::ObjectVariant, stream::StreamObject,
    };

    use super::*;

    #[test]
    fn fallback_salvages_valid_metadata_independently() {
        let to_unicode = ObjectVariant::Stream(StreamObject::new(
            1,
            0,
            Box::new(Dictionary::new(BTreeMap::new())),
            b"beginbfchar\n<41> <0042>\nendbfchar\n".to_vec(),
        ));
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                "BaseFont".to_string(),
                ObjectVariant::Name(b"Helvetica-Bold".to_vec()),
            ),
            ("FontDescriptor".to_string(), ObjectVariant::Integer(1)),
            ("FirstChar".to_string(), ObjectVariant::Integer(65)),
            ("LastChar".to_string(), ObjectVariant::Integer(65)),
            (
                "Widths".to_string(),
                ObjectVariant::Array(vec![ObjectVariant::Integer(625)]),
            ),
            ("Encoding".to_string(), ObjectVariant::Integer(1)),
            ("ToUnicode".to_string(), to_unicode),
        ]));

        let font = fallback_true_type_from_dictionary(&dictionary, &PassthroughResolver);

        assert_eq!(font.standard14, Some(Standard14Font::HelveticaBold));
        assert!(font.flags.is_empty());
        assert_eq!(
            font.widths.as_ref().and_then(|widths| widths.get(&65)),
            Some(&625.0)
        );
        assert_eq!(
            font.encoding
                .as_ref()
                .and_then(|encoding| encoding.names.get(65))
                .map(std::borrow::Cow::as_ref),
            Some("A")
        );
        assert_eq!(
            font.to_unicode
                .as_ref()
                .and_then(|cmap| cmap.map_char_code(0x41)),
            Some(['B'].as_slice())
        );
    }

    #[test]
    fn fallback_ignores_malformed_widths_to_unicode_and_cid_info() {
        let malformed_to_unicode = ObjectVariant::Stream(StreamObject::new(
            1,
            0,
            Box::new(Dictionary::new(BTreeMap::new())),
            b">".to_vec(),
        ));
        let dictionary = Dictionary::new(BTreeMap::from([
            ("Widths".to_string(), ObjectVariant::Integer(1)),
            ("ToUnicode".to_string(), malformed_to_unicode),
            ("CIDSystemInfo".to_string(), ObjectVariant::Integer(1)),
        ]));

        let font = fallback_true_type_from_dictionary(&dictionary, &PassthroughResolver);

        assert!(font.widths.is_none());
        assert!(font.to_unicode.is_none());
        assert_eq!(font.standard14, Some(Standard14Font::Helvetica));
        assert_eq!(
            font.font_file.as_ref(),
            Standard14Font::Helvetica.fallback_font_bytes()
        );
    }

    #[test]
    fn fallback_uses_cjk_program_for_known_cid_ordering() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            "CIDSystemInfo".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                "Ordering".to_string(),
                ObjectVariant::LiteralString(b"Japan1".to_vec()),
            )])))),
        )]));

        let fallback = fallback_true_type_from_dictionary(&dictionary, &PassthroughResolver);

        assert_eq!(fallback.font_file.as_ref(), NOTO_SANS_CJK_JP_REGULAR);
    }
}
