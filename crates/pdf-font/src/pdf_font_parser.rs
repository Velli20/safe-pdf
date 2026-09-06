//! Shared readers for font names and optional Unicode mappings.

use crate::pdf::ToUnicodeMap;
use pdf_cmap::{IdentityToUnicodeMap, ToUnicodeCMap};
use pdf_object_reader::{
    DictionaryContext, ObjectAccess, ReadResult, object_variant::ObjectVariant,
    resolved_object::ResolvedObject,
};
use std::sync::Arc;

/// Ignores unreadable entries and unsupported names, but propagates malformed CMap data.
pub(crate) fn to_unicode(
    context: &mut DictionaryContext<'_, impl ObjectAccess + ?Sized>,
) -> ReadResult<Option<Arc<dyn ToUnicodeMap>>> {
    let Some(value) = context
        .optional::<ResolvedObject>(b"ToUnicode")
        .ok()
        .flatten()
    else {
        return Ok(None);
    };
    if matches!(value.value(), ObjectVariant::Stream(_)) {
        let map = context.required::<ToUnicodeCMap>(b"ToUnicode")?;
        return Ok(Some(Arc::new(map)));
    }
    Ok(value
        .value()
        .try_bytes(context.source())
        .ok()
        .and_then(IdentityToUnicodeMap::from_name)
        .map(|map| {
            let map: Arc<dyn ToUnicodeMap> = Arc::new(map);
            map
        }))
}

/// BaseFont is a best-effort matching hint, including when constructing a fallback.
pub(crate) fn base_font(
    context: &mut DictionaryContext<'_, impl ObjectAccess + ?Sized>,
) -> Option<Arc<[u8]>> {
    context.optional(b"BaseFont").ok().flatten()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]
mod tests {
    use super::*;
    use pdf_cmap::{PdfCode, UnicodeSequence};
    use pdf_object_reader::{
        FromPdfObject, ObjectContext, ObjectReader, dictionary::Dictionary,
        object_resolver::PassthroughResolver,
    };

    struct UnicodeFixture(Option<Arc<dyn ToUnicodeMap>>);
    impl FromPdfObject for UnicodeFixture {
        fn from_pdf_object(
            context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
        ) -> ReadResult<Self> {
            to_unicode(&mut context.dictionary()?).map(Self)
        }
    }
    #[test]
    fn named_identity_to_unicode_map_is_supported() {
        let dictionary = Dictionary::from_entries([(
            b"ToUnicode".as_slice(),
            ObjectVariant::Name(b"Identity-H".to_vec()),
        )]);
        let map = ObjectReader::new(PassthroughResolver)
            .read::<UnicodeFixture>(&ObjectVariant::Dictionary(dictionary))
            .map(|value| value.0)
            .expect("the identity map should parse")
            .expect("the identity map should be present");
        let code = PdfCode::new(0x11B, 2).expect("the Czech character code should be valid");

        assert_eq!(map.map(code), Some(UnicodeSequence::from('ě')));
    }

    #[test]
    fn unsupported_named_to_unicode_map_is_ignored() {
        let dictionary = Dictionary::from_entries([(
            b"ToUnicode".as_slice(),
            ObjectVariant::Name(b"Unsupported-H".to_vec()),
        )]);

        assert!(
            ObjectReader::new(PassthroughResolver)
                .read::<UnicodeFixture>(&ObjectVariant::Dictionary(dictionary))
                .map(|value| value.0)
                .expect("unsupported names should remain non-fatal")
                .is_none()
        );
    }
}
