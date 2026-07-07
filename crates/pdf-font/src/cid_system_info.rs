use pdf_cmap::predefined::CidOrdering;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::error::FontError;

/// Extract a supported CID ordering from a font dictionary's `/CIDSystemInfo`.
///
/// # Paramaters
///
/// - `dictionary`: The PDF font dictionary that may contain `/CIDSystemInfo`.
/// - `objects`: The resolver used to dereference indirect PDF objects.
///
/// # Returns
///
/// A known [`CidOrdering`] when `/CIDSystemInfo /Ordering` is present and
/// supported, or `None` when the entry is absent or unknown.
pub(crate) fn cid_ordering_from_dictionary(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Option<CidOrdering>, FontError> {
    let Some(cid_system_info) = dictionary.optional_dictionary("CIDSystemInfo", objects)? else {
        return Ok(None);
    };

    let Some(ordering) = cid_system_info.optional_str("Ordering", objects)? else {
        return Ok(None);
    };

    Ok(CidOrdering::from_name(ordering))
}
