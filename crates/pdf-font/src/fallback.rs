use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{
    cid_system_info::CidOrdering, flags::FontFlags, standard14::Standard14Font,
    true_type_font::TrueTypeFont,
};

const NOTO_SANS_CJK_JP_REGULAR: &[u8] = include_bytes!("../assets/NotoSansCJKjp-Regular.otf");

/// Build a synthetic TrueType font from fallback font data.
///
/// # Paramaters
///
/// - `dictionary`: The PDF font dictionary used to select a bundled font program.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// A [`TrueTypeFont`] backed only by bundled fallback font data. PDF widths,
/// encoding, ToUnicode data, and descriptor flags are intentionally discarded.
pub(crate) fn fallback_true_type_from_dictionary(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> TrueTypeFont {
    let metadata = fallback_metadata_dictionary(dictionary, objects);
    let flags = FontFlags::from_dictionary(metadata, objects).unwrap_or_default();
    let standard14 = Standard14Font::from_dictionary(metadata, objects, flags);
    let font_file = if is_cjk_cid_font(metadata, objects) {
        NOTO_SANS_CJK_JP_REGULAR
    } else {
        standard14.fallback_font_bytes()
    };

    TrueTypeFont::from_bytes(font_file, Some(standard14))
}

/// Select metadata from a Type0 descendant when one is readable.
fn fallback_metadata_dictionary<'a>(
    dictionary: &'a Dictionary,
    objects: &'a dyn ObjectResolver,
) -> &'a Dictionary {
    dictionary
        .get("DescendantFonts")
        .and_then(|value| value.try_array(objects).ok())
        .and_then(|descendants| descendants.first())
        .and_then(|descendant| descendant.try_dictionary(objects).ok())
        .unwrap_or(dictionary)
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

    use pdf_object::{object_resolver::PassthroughResolver, object_variant::ObjectVariant};

    use super::*;

    #[test]
    fn fallback_discards_pdf_font_metadata() {
        let descriptor = Dictionary::new(BTreeMap::from([(
            "Flags".to_string(),
            ObjectVariant::Integer(i64::from(FontFlags::SYMBOLIC.bits())),
        )]));
        let dictionary = Dictionary::new(BTreeMap::from([
            (
                "BaseFont".to_string(),
                ObjectVariant::Name(b"Helvetica-Bold".to_vec()),
            ),
            (
                "FontDescriptor".to_string(),
                ObjectVariant::Dictionary(Box::new(descriptor)),
            ),
            ("FirstChar".to_string(), ObjectVariant::Integer(65)),
            ("LastChar".to_string(), ObjectVariant::Integer(65)),
            (
                "Widths".to_string(),
                ObjectVariant::Array(vec![ObjectVariant::Integer(625)]),
            ),
            (
                "Encoding".to_string(),
                ObjectVariant::Name(b"WinAnsiEncoding".to_vec()),
            ),
            ("ToUnicode".to_string(), ObjectVariant::Integer(1)),
        ]));

        let font = fallback_true_type_from_dictionary(&dictionary, &PassthroughResolver);

        assert_eq!(font.standard14, Some(Standard14Font::HelveticaBold));
        assert!(font.flags.is_empty());
        assert!(font.widths.is_none());
        assert!(font.encoding.is_none());
        assert!(font.to_unicode.is_none());
    }

    #[test]
    fn fallback_tolerates_malformed_selection_metadata() {
        let dictionary = Dictionary::new(BTreeMap::from([
            ("FontDescriptor".to_string(), ObjectVariant::Integer(1)),
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
    fn fallback_uses_cjk_program_from_type0_descendant() {
        let descendant = Dictionary::new(BTreeMap::from([(
            "CIDSystemInfo".to_string(),
            ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                "Ordering".to_string(),
                ObjectVariant::LiteralString(b"Japan1".to_vec()),
            )])))),
        )]));
        let dictionary = Dictionary::new(BTreeMap::from([(
            "DescendantFonts".to_string(),
            ObjectVariant::Array(vec![ObjectVariant::Dictionary(Box::new(descendant))]),
        )]));

        let fallback = fallback_true_type_from_dictionary(&dictionary, &PassthroughResolver);

        assert_eq!(fallback.font_file.as_ref(), NOTO_SANS_CJK_JP_REGULAR);
    }
}
